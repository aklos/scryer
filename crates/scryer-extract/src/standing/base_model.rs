//! The model a checkout's changes are read against: the tree derived from the
//! files at its change base, HEAD on the default branch, the merge base off
//! it. [composition]
//!
//! A base is a commit, so its model never changes: it is derived once from a
//! private export of that commit's files and cached by git tree outside the
//! project, under `~/.scryer/bases`. Each read exports into a directory of
//! its own, so reads of one commit at once never clobber each other. The
//! export is named after the project's
//! own directory, because the system's name, and with it ids, come from that
//! name.

use crate::tree_from_disk::{derived_tree_uncached, DERIVE_VERSION};
use scryer_core::worktree::{bases_root, export_revision, project_slug, tree_of};
use std::sync::atomic::{AtomicU64, Ordering};
use scryer_core::ScryModel;
use std::path::Path;

pub use scryer_core::worktree::{change_base, ChangeBase};

static NEXT: AtomicU64 = AtomicU64::new(0);

/// The model at `commit`, every claim todo tests included, as the project's
/// tree reads it.
pub fn model_at(project: &Path, commit: &str) -> Result<ScryModel, String> {
    model_at_in(project, commit, &bases_root())
}

/// [`model_at`] with its cache under `root`.
pub fn model_at_in(project: &Path, commit: &str, root: &Path) -> Result<ScryModel, String> {
    let dir = root.join(project_slug(project));
    let tree = tree_of(project, commit)?;
    let cached = dir.join(format!("{tree}-v{DERIVE_VERSION}.json"));
    if let Some(model) = std::fs::read(&cached).ok().and_then(|b| serde_json::from_slice(&b).ok()) {
        return Ok(model);
    }
    // Each read exports into a directory of its own and writes the cache by
    // rename: two reads of one commit at once (the top bar's base and the
    // activity feed) must never delete each other's files or read each
    // other's half-written cache.
    let own = format!("{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed));
    let work = dir.join(format!("export-{tree}-{own}"));
    let name = project.file_name().map_or_else(|| "project".into(), |n| n.to_os_string());
    let derived = export_revision(project, commit, &work.join(&name)).and_then(|_| derived_tree_uncached(&work.join(&name)));
    let _ = std::fs::remove_dir_all(&work);
    let model = derived?.planned;
    // A failed cache write only costs the next read a derivation.
    if let Ok(bytes) = serde_json::to_vec(&model) {
        let part = dir.join(format!("{tree}-{own}.part"));
        let _ = std::fs::create_dir_all(&dir)
            .and_then(|_| std::fs::write(&part, bytes))
            .and_then(|_| std::fs::rename(&part, &cached));
    }
    Ok(model)
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

    fn claims(m: &ScryModel) -> Vec<String> {
        let mut out: Vec<String> = m.nodes.iter().flat_map(|n| n.claims.iter().map(|c| c.statement.clone())).collect();
        out.sort();
        out
    }

    const LIB: &str = "//! Notes.\n\npub fn one() -> u32 { 1 }\n\n#[cfg(test)]\nmod tests {\n    /// Returns one.\n    #[test]\n    fn returns_one() { assert_eq!(super::one(), 1); }\n}\n";

    /// Derives the model at the base commit from that commit's files, not the working tree, with ids matching the working tree's.
    #[test]
    fn derives_the_model_at_a_commit_from_its_files() {
        let root = tempfile::tempdir().unwrap();
        let p = root.path().join("notes");
        std::fs::create_dir_all(&p).unwrap();
        git(&p, &["init", "-q", "-b", "main"]);
        write(&p, "Cargo.toml", "[package]\nname = \"notes\"\nversion = \"0.1.0\"\nedition = \"2021\"\n");
        write(&p, "src/lib.rs", LIB);
        git(&p, &["add", "-A"]);
        git(&p, &["commit", "-qm", "init"]);
        let at_head = crate::tree_from_disk::derived_tree_uncached(&p).unwrap().planned;
        write(&p, "src/lib.rs", &LIB.replace("Returns one.", "Returns the number one."));
        let cache = tempfile::tempdir().unwrap();
        let base = model_at_in(&p, "HEAD", cache.path()).unwrap();
        assert_eq!(claims(&base), ["Returns one."]);
        let ids = |m: &ScryModel| m.nodes.iter().map(|n| n.id.clone()).collect::<Vec<_>>();
        assert_eq!(ids(&base), ids(&at_head));
        assert!(std::fs::read_to_string(p.join("src/lib.rs")).unwrap().contains("the number one"), "the checkout is untouched");
    }

    /// When two reads derive the same base at once, each gets the whole model.
    #[test]
    fn concurrent_reads_of_one_base_each_get_the_whole_model() {
        let root = tempfile::tempdir().unwrap();
        let p = root.path().join("notes");
        std::fs::create_dir_all(&p).unwrap();
        git(&p, &["init", "-q", "-b", "main"]);
        write(&p, "Cargo.toml", "[package]\nname = \"notes\"\nversion = \"0.1.0\"\nedition = \"2021\"\n");
        for i in 0..40 {
            write(&p, &format!("src/m{i}.rs"), &LIB.replace("Returns one.", &format!("Returns one, {i}.")));
        }
        write(&p, "src/lib.rs", &(0..40).map(|i| format!("pub mod m{i};\n")).collect::<String>());
        git(&p, &["add", "-A"]);
        git(&p, &["commit", "-qm", "init"]);
        let cache = tempfile::tempdir().unwrap();
        let counts: Vec<usize> = std::thread::scope(|s| {
            let jobs: Vec<_> = (0..4).map(|_| s.spawn(|| claims(&model_at_in(&p, "HEAD", cache.path()).unwrap()).len())).collect();
            jobs.into_iter().map(|j| j.join().unwrap()).collect()
        });
        assert_eq!(counts, [40, 40, 40, 40]);
        assert_eq!(claims(&model_at_in(&p, "HEAD", cache.path()).unwrap()).len(), 40, "the cache holds the whole model");
    }

    /// When a base's tree was derived before, reads its model back without deriving it again.
    #[test]
    fn reads_a_derived_base_back_from_its_cache() {
        let root = tempfile::tempdir().unwrap();
        let p = root.path().join("notes");
        std::fs::create_dir_all(&p).unwrap();
        git(&p, &["init", "-q", "-b", "main"]);
        write(&p, "Cargo.toml", "[package]\nname = \"notes\"\nversion = \"0.1.0\"\nedition = \"2021\"\n");
        write(&p, "src/lib.rs", LIB);
        git(&p, &["add", "-A"]);
        git(&p, &["commit", "-qm", "init"]);
        let cache = tempfile::tempdir().unwrap();
        let first = model_at_in(&p, "HEAD", cache.path()).unwrap();
        let file = std::fs::read_dir(cache.path().join(project_slug(&p))).unwrap().next().unwrap().unwrap().path();
        let mut cached = first.clone();
        cached.nodes[0].name = "From the cache".into();
        std::fs::write(&file, serde_json::to_vec(&cached).unwrap()).unwrap();
        assert_eq!(model_at_in(&p, "HEAD", cache.path()).unwrap().nodes[0].name, "From the cache");
    }
}
