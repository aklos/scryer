//! What changed since you last looked, across the project's live git
//! worktrees: each worktree's sessions read against that checkout's own
//! derived tree and status (claims and tests live in the tree, so status is
//! per checkout), merged newest first with each session labelled by its
//! worktree. A session logged in this checkout whose every touch falls
//! inside a nested worktree is that worktree's.

use crate::composition::test_status::derived_status;
use scryer_core::derived_tree::{
    landing, landing_at, merge_worktrees, sessions_since, take_prefixed, Landing, LandingWorktree,
};
use scryer_core::ModelRef;
use std::path::{Path, PathBuf};

/// The landing view for `project` since `since`, over every live worktree.
pub fn landing_across_worktrees(project: &Path, since: u64) -> Result<Landing, String> {
    let r = ModelRef::ProjectLocal(project.to_path_buf());
    let worktrees = scryer_core::worktree::list_worktrees(project);
    if worktrees.len() <= 1 {
        let (tree, status) = derived_status(&r)?;
        let mut out = landing_at(&r, &tree, &status, since);
        out.branch = worktrees.into_iter().next().and_then(|w| w.branch);
        return Ok(out);
    }
    let canon = |p: &Path| p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
    let here = canon(project);
    let mut buckets: Vec<(LandingWorktree, PathBuf, Vec<_>)> = worktrees
        .iter()
        .map(|w| {
            let path = canon(&w.path);
            let sessions = sessions_since(&ModelRef::ProjectLocal(path.clone()), since);
            let wt = LandingWorktree {
                path: path.display().to_string(),
                branch: w.branch.clone(),
                current: path == here,
                sessions: 0,
            };
            (wt, path, sessions)
        })
        .collect();
    // Sessions logged in an enclosing checkout that only touched a nested
    // worktree's files belong to that worktree.
    let nested: Vec<(usize, usize, String)> = (0..buckets.len())
        .flat_map(|i| (0..buckets.len()).map(move |j| (i, j)))
        .filter(|(i, j)| i != j)
        .filter_map(|(i, j)| {
            let rel = buckets[j].1.strip_prefix(&buckets[i].1).ok()?;
            (!rel.as_os_str().is_empty()).then(|| (i, j, format!("{}/", rel.to_string_lossy().replace('\\', "/"))))
        })
        .collect();
    for (i, j, prefix) in nested {
        let moved = take_prefixed(&mut buckets[i].2, &prefix);
        buckets[j].2.extend(moved);
    }
    let parts = buckets
        .into_iter()
        .map(|(wt, path, sessions)| {
            let part = if sessions.is_empty() {
                Landing::default()
            } else {
                // A checkout whose tree cannot be read shows no sessions
                // rather than failing the whole page.
                derived_status(&ModelRef::ProjectLocal(path))
                    .map(|(tree, status)| landing(&sessions, &tree, &status, since))
                    .unwrap_or_default()
            };
            (wt, part)
        })
        .collect();
    let mut out = merge_worktrees(since, parts);
    out.branch = out.worktrees.iter().find(|w| w.current).and_then(|w| w.branch.clone());
    Ok(out)
}

#[cfg(test)]
mod tests {
    use scryer_core::session::record_touch;
    use scryer_core::ModelRef;
    use std::path::Path;
    use std::process::Command;

    fn git(dir: &Path, args: &[&str]) {
        let out = Command::new("git").args(args).current_dir(dir).output().unwrap();
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    }

    fn write(dir: &Path, rel: &str, text: &str) {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    /// When the project has several worktrees, each session is labelled with
    /// the worktree it worked in (a session logged in the main checkout that
    /// touched only a nested worktree's files is the nested one's), a worktree
    /// with no sessions is listed by its branch, and a component touched in
    /// two worktrees counts as shared.
    #[test]
    fn sessions_group_by_worktree_and_share_across_them() {
        let dir = tempfile::tempdir().unwrap();
        let main = dir.path().join("repo");
        write(&main, "Cargo.toml", "[package]\nname = \"core\"\nversion = \"0.1.0\"\n");
        write(&main, "src/lib.rs", "pub mod draft;\n");
        write(&main, "src/draft.rs", "pub fn save() {}\n");
        git(&main, &["init", "-q", "-b", "main"]);
        git(&main, &["add", "-A"]);
        git(&main, &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-qm", "init"]);
        let side = dir.path().join("side");
        let idle = dir.path().join("idle");
        git(&main, &["worktree", "add", "-q", "-b", "side", side.to_str().unwrap()]);
        git(&main, &["worktree", "add", "-q", "-b", "idle", idle.to_str().unwrap()]);
        git(&main, &["worktree", "add", "-q", "-b", "nested", "wt/nested"]);

        let at = |p: &Path| ModelRef::ProjectLocal(p.to_path_buf());
        record_touch(&at(&main), "s-main", "src/draft.rs").unwrap();
        record_touch(&at(&side), "s-side", "src/draft.rs").unwrap();
        record_touch(&at(&main), "s-nested", "wt/nested/src/lib.rs").unwrap();

        let landing = super::landing_across_worktrees(&main, 0).unwrap();
        let canon = |p: &Path| p.canonicalize().unwrap().display().to_string();
        let worktree_of = |id: &str| landing.sessions.iter().find(|s| s.session == id).unwrap().worktree.clone();
        assert_eq!(worktree_of("s-main"), Some(canon(&main)));
        assert_eq!(worktree_of("s-side"), Some(canon(&side)));
        assert_eq!(worktree_of("s-nested"), Some(canon(&main.join("wt/nested"))));

        let listed: Vec<(Option<&str>, usize, bool)> =
            landing.worktrees.iter().map(|w| (w.branch.as_deref(), w.sessions, w.current)).collect();
        // Main checkout first, then git's own order (by path).
        assert_eq!(
            listed,
            vec![(Some("main"), 1, true), (Some("idle"), 0, false), (Some("nested"), 1, false), (Some("side"), 1, false)]
        );
        assert!(
            landing.shared.iter().any(|c| c.sessions == vec!["s-main".to_string(), "s-side".to_string()]),
            "{:?}",
            landing.shared
        );
    }
}
