//! The project's activity, read the way a forge reads a repository: its
//! commits newest first, each with what it changed in the model, and what
//! is uncommitted on top of them. [composition]
//!
//! A commit's diff is its first parent's model against its own, both from
//! [`model_at`], so each commit is derived once and read from the cache
//! after. The uncommitted diff is HEAD's model against the working tree's.

use crate::standing::base_model::model_at;
use crate::tree_from_disk::derived_tree;
use scryer_core::model_diff::{model_diff, ModelDiff};
use scryer_core::ScryModel;
use std::path::Path;

pub use scryer_core::worktree::{history, Commit};

/// What `commit` changed in the model against its first parent; the first
/// commit, against nothing.
pub fn commit_diff(project: &Path, commit: &str, parent: Option<&str>) -> Result<ModelDiff, String> {
    let before = match parent {
        Some(p) => model_at(project, p)?,
        None => ScryModel::default(),
    };
    Ok(model_diff(&before, &model_at(project, commit)?))
}

/// What the working tree changed in the model against HEAD.
pub fn uncommitted_diff(project: &Path) -> Result<ModelDiff, String> {
    let head = model_at(project, "HEAD")?;
    Ok(model_diff(&head, &derived_tree(project)?.planned))
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

    fn lib(claim: &str) -> String {
        format!("//! Notes.\n\npub fn one() -> u32 {{ 1 }}\n\n#[cfg(test)]\nmod tests {{\n    /// {claim}\n    #[test]\n    fn returns_one() {{ assert_eq!(super::one(), 1); }}\n}}\n")
    }

    fn claims(d: &ModelDiff) -> Vec<String> {
        d.nodes.iter().flat_map(|n| n.claims.iter().map(|c| format!("{:?} {}", c.change, c.statement))).collect()
    }

    /// Reads each commit's model changes against its first parent, and the working tree's against HEAD.
    #[test]
    fn reads_each_commits_changes_and_the_uncommitted_ones() {
        let root = tempfile::tempdir().unwrap();
        std::env::set_var("SCRYER_BASES_DIR", root.path().join("bases"));
        let p = root.path().join("notes");
        std::fs::create_dir_all(&p).unwrap();
        git(&p, &["init", "-q", "-b", "main"]);
        write(&p, "Cargo.toml", "[package]\nname = \"notes\"\nversion = \"0.1.0\"\nedition = \"2021\"\n");
        write(&p, "src/lib.rs", &lib("Returns one."));
        git(&p, &["add", "-A"]);
        git(&p, &["commit", "-qm", "init"]);
        write(&p, "src/lib.rs", &lib("Returns the number one."));
        git(&p, &["commit", "-qam", "reword"]);
        write(&p, "src/lib.rs", &lib("Returns 1."));

        let log = history(&p, 0, 10).unwrap();
        assert_eq!(log.iter().map(|c| c.subject.as_str()).collect::<Vec<_>>(), ["reword", "init"]);
        let reword = commit_diff(&p, &log[0].sha, log[0].parent.as_deref()).unwrap();
        assert_eq!(claims(&reword), ["Modified Returns the number one."]);
        let init = commit_diff(&p, &log[1].sha, None).unwrap();
        assert_eq!(claims(&init), ["Added Returns one."]);
        assert_eq!(claims(&uncommitted_diff(&p).unwrap()), ["Modified Returns 1."]);
    }
}
