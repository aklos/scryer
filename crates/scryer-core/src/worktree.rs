//! The isolated tree a falsification probe mutates.
//!
//! A probe deliberately breaks code to ask whether the attached test notices.
//! Doing that in the developer's own working tree is hostile: their editor is
//! open on those files, their agent may be mid-edit, and a restore that races
//! either one loses work. So the mutation happens in a git worktree instead,
//! and the developer's tree is never touched at all.
//!
//! The worktree is REUSED, not created per probe. A fresh checkout starts with
//! a cold build — no `target/`, no `node_modules/`, no docker layers — and for
//! most projects that cold build dwarfs the test run the probe actually cares
//! about. Keeping one worktree per project means the first probe pays for the
//! build and every probe after it pays for an incremental one. `git clean -fd`
//! (deliberately without `-x`) drops stray files while leaving ignored build
//! output exactly where it is, which is what keeps it warm.
//!
//! Syncing copies the working tree, not the last commit. Mid-session the
//! interesting code is uncommitted almost by definition, and a probe against
//! `HEAD` would be answering a question about code the developer already moved
//! past.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::settings::global_dir;

/// Where probe worktrees live: `~/.scryer/probes` by default. Outside every
/// project, so a probe tree is never mistaken for the developer's own checkout
/// and never shows up in their file watcher, search results, or `git status`.
///
/// `SCRYER_PROBES_DIR` relocates them — for a faster disk, for a machine where
/// `$HOME` is small or networked, and for this crate's own tests, which must
/// not scatter worktrees through the home directory of whoever runs them.
fn probes_root() -> PathBuf {
    match std::env::var_os("SCRYER_PROBES_DIR") {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => global_dir().join("probes"),
    }
}

/// A stable directory name for one project's worktree: the project's own
/// directory name for legibility, plus a hash of its absolute path so two
/// checkouts of the same repo never collide. FNV-1a 64, matching the rest of
/// the codebase — std's `DefaultHasher` is documented unstable across
/// releases, and an unstable name would orphan a warm worktree on every
/// toolchain bump.
fn slug_for(project: &Path) -> String {
    let name = project
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "project".into());
    let safe: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
        .collect();
    let mut h: u64 = 0xcbf29ce484222325;
    for b in project.to_string_lossy().as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{safe}-{h:016x}")
}

/// Run a git command in `dir`, returning stdout on success and git's own
/// stderr on failure — the caller surfaces it verbatim rather than inventing a
/// diagnosis for a tool that already explained itself.
fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .map_err(|e| format!("could not run git: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

/// Is `project` inside a git work tree? The probe has no fallback if it isn't:
/// mutating the developer's own files is the exact thing the worktree exists
/// to prevent, so a non-repo is a refusal, never a downgrade.
pub fn is_git_repo(project: &Path) -> bool {
    matches!(
        git(project, &["rev-parse", "--is-inside-work-tree"]),
        Ok(s) if s.trim() == "true"
    )
}

/// The worktree path for a project, whether or not it exists yet.
pub fn worktree_path(project: &Path) -> PathBuf {
    probes_root().join(slug_for(project))
}

/// Hand back a worktree synced to the project's CURRENT state — commits plus
/// uncommitted and untracked edits — creating it on first use and reusing it
/// (build cache and all) every time after.
///
/// Returns the worktree root. Refuses when the project is not a git repository.
pub fn ensure_synced(project: &Path) -> Result<PathBuf, String> {
    if !is_git_repo(project) {
        return Err(format!(
            "{} is not a git repository — a probe mutates code to see whether the \
             test notices, and without a worktree to do that in it would have to \
             break your own working tree. Initialise git first.",
            project.display()
        ));
    }
    let wt = worktree_path(project);
    if !wt.join(".git").exists() {
        create(project, &wt)?;
    }
    sync(project, &wt)?;
    Ok(wt)
}

/// Create the worktree. Detached, so it never competes with the developer for
/// a branch, and `--force` because a stale administrative entry from a
/// worktree whose directory was deleted out from under git must not be a
/// permanent wedge.
fn create(project: &Path, wt: &Path) -> Result<(), String> {
    if let Some(parent) = wt.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("could not create {}: {e}", parent.display()))?;
    }
    // A directory left behind without git's record of it (an interrupted
    // delete, a manual rm of ~/.scryer) would make `worktree add` refuse.
    if wt.exists() {
        std::fs::remove_dir_all(wt)
            .map_err(|e| format!("could not clear stale worktree {}: {e}", wt.display()))?;
    }
    let _ = git(project, &["worktree", "prune"]);
    let path = wt.to_string_lossy().to_string();
    git(project, &["worktree", "add", "--detach", "--force", &path, "HEAD"])?;
    Ok(())
}

/// Bring the worktree to the project's current state.
///
/// Order matters: discard whatever the last probe left, align to the same
/// commit the developer is on, then replay their uncommitted work on top. The
/// `clean` deliberately omits `-x`, so ignored build output survives and the
/// next test run is incremental.
fn sync(project: &Path, wt: &Path) -> Result<(), String> {
    let head = git(project, &["rev-parse", "HEAD"])?.trim().to_string();

    git(wt, &["reset", "--hard"])?;
    git(wt, &["clean", "-fd"])?;
    git(wt, &["checkout", "--detach", &head])?;

    // Tracked changes: `diff HEAD` covers staged and unstaged together, and
    // `--binary` keeps non-text edits applicable.
    let diff = git(project, &["diff", "--binary", "HEAD"])?;
    if !diff.trim().is_empty() {
        apply_patch(wt, &diff)?;
    }

    // Untracked-but-not-ignored files are part of the working tree the
    // developer sees, so a probe that ignored them would be reading a
    // different codebase than the one on screen.
    for rel in git(project, &["ls-files", "--others", "--exclude-standard"])?.lines() {
        let rel = rel.trim();
        if rel.is_empty() {
            continue;
        }
        let src = project.join(rel);
        let dst = wt.join(rel);
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("could not create {}: {e}", parent.display()))?;
        }
        std::fs::copy(&src, &dst)
            .map_err(|e| format!("could not copy {rel} into the probe worktree: {e}"))?;
    }
    Ok(())
}

/// Feed a patch to `git apply` on stdin.
fn apply_patch(wt: &Path, diff: &str) -> Result<(), String> {
    use std::io::Write;
    let mut child = Command::new("git")
        .args(["apply", "--whitespace=nowarn", "-"])
        .current_dir(wt)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("could not run git apply: {e}"))?;
    child
        .stdin
        .as_mut()
        .ok_or("git apply took no stdin")?
        .write_all(diff.as_bytes())
        .map_err(|e| format!("could not write the patch to git apply: {e}"))?;
    let out = child
        .wait_with_output()
        .map_err(|e| format!("git apply did not finish: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "could not replay your uncommitted changes into the probe worktree: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(())
}

/// Drop whatever the probe did. Called when a probe ends, so a mutation never
/// survives into the next round and a survivor is never mistaken for a break
/// the previous probe left behind. Ignored build output is preserved, as in
/// `sync`.
pub fn reset(project: &Path) -> Result<(), String> {
    let wt = worktree_path(project);
    if !wt.join(".git").exists() {
        return Ok(());
    }
    git(&wt, &["reset", "--hard"])?;
    git(&wt, &["clean", "-fd"])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Point every test's worktrees at a scratch directory. Set once, before
    /// any test reads it, because `set_var` is process-global and these tests
    /// run in parallel — and because scattering worktrees through the home
    /// directory of whoever runs the suite is not acceptable collateral.
    fn isolate_probes_root() {
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(|| {
            // Never clear the root here: nextest gives each test its own
            // process, so a wipe would race sibling tests already using it.
            // Slugs are unique per fixture, and /tmp is the OS's to reap.
            std::env::set_var("SCRYER_PROBES_DIR", std::env::temp_dir().join("scryer-worktree-tests"));
        });
    }

    /// A real git repo with one commit — the probe worktree machinery is all
    /// git behaviour, so a fake would be testing the mock.
    fn repo() -> tempfile::TempDir {
        isolate_probes_root();
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        git(p, &["init", "-q"]).unwrap();
        git(p, &["config", "user.email", "t@t.t"]).unwrap();
        git(p, &["config", "user.name", "t"]).unwrap();
        fs::write(p.join(".gitignore"), "build/\n").unwrap();
        fs::write(p.join("m.rs"), "fn main() { println!(\"one\"); }\n").unwrap();
        git(p, &["add", "-A"]).unwrap();
        git(p, &["commit", "-qm", "init"]).unwrap();
        dir
    }

    /// Worktrees are keyed by absolute path, so the same repo name in two
    /// places never shares (and corrupts) one tree.
    #[test]
    fn two_checkouts_of_one_name_get_separate_worktrees() {
        assert_ne!(slug_for(Path::new("/a/scryer")), slug_for(Path::new("/b/scryer")));
        assert!(slug_for(Path::new("/a/my proj")).starts_with("my-proj-"));
    }

    /// resp-763: a non-repo is refused outright. The fallback a probe would
    /// otherwise need — mutating the developer's own files — is the thing the
    /// worktree exists to prevent.
    #[test]
    fn a_project_without_git_is_refused_not_downgraded() {
        isolate_probes_root();
        let dir = tempfile::tempdir().unwrap();
        let err = ensure_synced(dir.path()).unwrap_err();
        assert!(err.contains("not a git repository"), "{err}");
    }

    /// resp-761: the probe must see the code as it stands, so uncommitted
    /// edits and brand-new files both have to make the crossing.
    #[test]
    fn syncing_carries_uncommitted_and_untracked_work_across() {
        let dir = repo();
        let p = dir.path();
        fs::write(p.join("m.rs"), "fn main() { println!(\"two\"); }\n").unwrap();
        fs::write(p.join("new.rs"), "// brand new\n").unwrap();

        let wt = ensure_synced(p).unwrap();

        assert!(fs::read_to_string(wt.join("m.rs")).unwrap().contains("two"));
        assert!(fs::read_to_string(wt.join("new.rs")).unwrap().contains("brand new"));
        fs::remove_dir_all(&wt).ok();
    }

    /// resp-760 and resp-762: the same worktree comes back, its ignored build
    /// output intact — that reuse is the whole reason a probe costs an
    /// incremental run — and the previous round's mutation does not.
    #[test]
    fn the_worktree_is_reused_warm_and_the_last_mutation_is_gone() {
        let dir = repo();
        let p = dir.path();
        let first = ensure_synced(p).unwrap();
        fs::create_dir_all(first.join("build")).unwrap();
        fs::write(first.join("build/cache.bin"), "warm").unwrap();

        // A probe breaks the code, then the round ends.
        fs::write(first.join("m.rs"), "fn main() { panic!() }\n").unwrap();
        reset(p).unwrap();

        let second = ensure_synced(p).unwrap();
        assert_eq!(first, second, "one worktree per project, reused");
        assert!(second.join("build/cache.bin").exists(), "the build cache survives");
        assert!(
            fs::read_to_string(second.join("m.rs")).unwrap().contains("one"),
            "the mutation does not"
        );
        fs::remove_dir_all(&second).ok();
    }

    /// A worktree directory deleted out from under git (a cleaned ~/.scryer)
    /// must not wedge every future probe.
    #[test]
    fn a_deleted_worktree_directory_is_rebuilt_not_a_permanent_error() {
        let dir = repo();
        let p = dir.path();
        let wt = ensure_synced(p).unwrap();
        fs::remove_dir_all(&wt).unwrap();

        let again = ensure_synced(p).unwrap();
        assert!(again.join("m.rs").exists(), "rebuilt after the directory vanished");
        fs::remove_dir_all(&again).ok();
    }
}
