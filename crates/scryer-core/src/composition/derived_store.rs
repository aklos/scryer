//! The derived tree, read-only. `scryer_extract::derived_tree` derives it and
//! writes it here; readers that cannot depend on the extractor read the last
//! derivation back. Behind `SCRYER_DERIVED_TREE=1` so the authored model stays
//! the default until the derived one replaces it.

use crate::domain::derived_tree::DerivedTree;
use crate::infrastructure::derived_tree_cache;
use crate::{ModelRef, ScryModel};

/// The environment switch that shows the derived tree beside the authored one.
pub const DERIVED_TREE_ENV: &str = "SCRYER_DERIVED_TREE";

/// Whether the derived tree is switched on for this process.
pub fn derived_tree_enabled() -> bool {
    std::env::var(DERIVED_TREE_ENV).is_ok_and(|v| v == "1" || v.eq_ignore_ascii_case("true"))
}

/// The last derived tree for this project, when one was written.
pub fn read_derived_tree_at(r: &ModelRef) -> Option<DerivedTree> {
    derived_tree_cache::read_derived_file(&r.derived_tree_path())
}

/// The derived tree's planned layer, shaped like `read_planned_at` returns
/// the authored one, so the app and MCP read it unchanged.
pub fn read_derived_planned_at(r: &ModelRef) -> Result<ScryModel, String> {
    read_derived_tree_at(r)
        .map(|t| t.planned)
        .ok_or_else(|| format!("no derived tree for '{}' yet", r.display_name()))
}

/// The derived tree's committed layer, shaped like `read_model_at`.
pub fn read_derived_committed_at(r: &ModelRef) -> Result<ScryModel, String> {
    read_derived_tree_at(r)
        .map(|t| t.committed)
        .ok_or_else(|| format!("no derived tree for '{}' yet", r.display_name()))
}

/// Store a derived tree for later reads.
pub fn write_derived_tree_at(r: &ModelRef, tree: &DerivedTree) -> Result<(), String> {
    derived_tree_cache::write_derived_file(&r.derived_tree_path(), tree)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// When a derived tree has been written, reads return each layer as stored.
    #[test]
    fn reads_back_the_written_layers() {
        let dir = tempfile::tempdir().unwrap();
        let r = ModelRef::ProjectLocal(dir.path().to_path_buf());
        assert!(read_derived_tree_at(&r).is_none());
        assert!(read_derived_planned_at(&r).is_err());

        let mut planned = ScryModel::new();
        planned.nodes.push(crate::Node {
            id: "node-a".into(),
            kind: crate::Kind::System,
            name: "sys".into(),
            parent_id: None,
            external: None,
            technology: None,
            description: None,
            vagrant: None,
            stale: None,
            responsibilities: Vec::new(),
            properties: Vec::new(),
            icon: None,
            notes: None,
            position: None,
            directives: Vec::new(),
            style: None,
            layer: None,
        });
        let tree = DerivedTree {
            fingerprint: 7,
            committed: ScryModel::new(),
            planned,
            claims: Vec::new(),
        };
        write_derived_tree_at(&r, &tree).unwrap();
        assert_eq!(read_derived_tree_at(&r).unwrap().fingerprint, 7);
        assert_eq!(read_derived_planned_at(&r).unwrap().nodes.len(), 1);
        assert!(read_derived_committed_at(&r).unwrap().nodes.is_empty());
    }
}
