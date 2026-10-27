//! Each of the project's git worktrees against its base branch, the main
//! checkout's branch: the files it changed since branching, read against
//! that checkout's own derived tree and status, and the components more
//! than one worktree changed. A checkout that changed nothing is not
//! derived; one that cannot be read carries its error instead of failing
//! the rest.

use crate::test_status::derived_status;
use scryer_core::worktree::{changes_against, fill_work, list_worktrees, overlaps, WorktreeWork, Worktrees};
use scryer_core::ModelRef;
use std::path::Path;

/// Every live worktree of `project`, its work and the overlaps between
/// them. Empty for a project with a single checkout.
pub fn worktrees_overview(project: &Path) -> Result<Worktrees, String> {
    let list = list_worktrees(project);
    if list.len() <= 1 {
        return Ok(Worktrees::default());
    }
    let canon = |p: &Path| p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
    let here = canon(project);
    let base = list[0].branch.clone();
    // Each checkout derives its own tree, which can take seconds: in parallel.
    let worktrees: Vec<WorktreeWork> = std::thread::scope(|scope| {
        let jobs: Vec<_> = list
            .iter()
            .map(|w| {
                let path = canon(&w.path);
                let base = base.clone().filter(|b| w.branch.as_ref() != Some(b));
                let mut work = WorktreeWork {
                    path: path.display().to_string(),
                    branch: w.branch.clone(),
                    current: path == here,
                    base,
                    ..WorktreeWork::default()
                };
                scope.spawn(move || {
                    let read = changes_against(&path, work.base.as_deref()).and_then(|ch| {
                        work.ahead = ch.ahead;
                        work.behind = ch.behind;
                        if ch.files.is_empty() {
                            return Ok(());
                        }
                        let (tree, status) = derived_status(&ModelRef::ProjectLocal(path.clone()))?;
                        fill_work(&mut work, ch.files, &tree, &status);
                        Ok(())
                    });
                    work.error = read.err();
                    work
                })
            })
            .collect();
        jobs.into_iter().filter_map(|j| j.join().ok()).collect()
    });
    let overlaps = overlaps(&worktrees);
    Ok(Worktrees { worktrees, overlaps })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn git(dir: &Path, args: &[&str]) {
        let out = Command::new("git")
            .args(["-c", "user.name=t", "-c", "user.email=t@t"])
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    }

    fn write(root: &Path, rel: &str, body: &str) {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
    }

    /// When the project has several worktrees, each lists the claims its
    /// changes since branching reach, a worktree that changed nothing is
    /// not ready, and a component two worktrees changed is an overlap.
    #[test]
    fn each_worktree_shows_the_claims_its_changes_reach_and_where_they_overlap() {
        let dir = tempfile::tempdir().unwrap();
        let main = dir.path().join("repo");
        write(&main, "Cargo.toml", "[package]\nname = \"notes\"\nversion = \"0.1.0\"\n");
        write(&main, "src/lib.rs", "//! Keeps notes.\npub mod store;\n");
        write(
            &main,
            "src/store.rs",
            "//! Persists notes.\npub fn put() -> bool { true }\n#[cfg(test)]\nmod tests {\n    /// Stores a note\n    #[test]\n    fn stores() { assert!(super::put()); }\n}\n",
        );
        write(&main, ".scryer/structure.toml", "");
        git(&main, &["init", "-q"]);
        git(&main, &["add", "."]);
        git(&main, &["commit", "-qm", "base"]);
        let a = dir.path().join("a");
        let b = dir.path().join("b");
        let idle = dir.path().join("idle");
        for wt in [&a, &b, &idle] {
            git(&main, &["worktree", "add", "-q", wt.to_str().unwrap()]);
        }
        for wt in [&a, &b] {
            write(wt, "src/store.rs", &std::fs::read_to_string(wt.join("src/store.rs")).unwrap().replace("true", "!false"));
        }
        let o = worktrees_overview(&a).unwrap();
        assert_eq!(o.worktrees.len(), 4);
        let at = |p: &Path| o.worktrees.iter().find(|w| Path::new(&w.path) == p.canonicalize().unwrap()).unwrap();
        assert!(at(&a).current);
        assert_eq!(at(&a).files, ["src/store.rs"]);
        assert_eq!(at(&a).claims.iter().map(|c| c.statement.as_str()).collect::<Vec<_>>(), ["Stores a note"]);
        assert!(!at(&a).ready, "the claim is unproven there");
        assert!(at(&idle).files.is_empty() && !at(&idle).ready);
        assert_eq!(o.overlaps.len(), 1);
        assert_eq!(o.overlaps[0].worktrees.len(), 2);
    }

    /// A project with one checkout has no worktrees to compare.
    #[test]
    fn a_single_checkout_has_no_overview() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(worktrees_overview(dir.path()).unwrap(), Worktrees::default());
    }
}
