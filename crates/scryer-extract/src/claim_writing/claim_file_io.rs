//! Claims written back to disk: a reworded description into its test file,
//! a new claim as a todo stub into the test file its node's tests live in,
//! a container's display name into `structure.toml`.

use crate::claim_text::{insert_stub, rewrite_description, snake_name, ClaimSite};
use crate::derivation::set_container_name;
use crate::tree_from_disk::derived_tree;
use scryer_core::derived_tree::DerivedTree;
use scryer_core::{Kind, Node};
use std::collections::BTreeMap;
use std::path::Path;

fn node<'t>(tree: &'t DerivedTree, id: &str) -> Result<&'t Node, String> {
    tree.planned
        .nodes
        .iter()
        .find(|n| n.id == id)
        .ok_or_else(|| format!("no node {id} in the derived tree"))
}

/// A container's directory, from its boundary glob.
fn container_dir(tree: &DerivedTree, container_id: &str) -> String {
    tree.planned
        .boundaries
        .get(container_id)
        .and_then(|b| b.first())
        .map(|s| s.pattern.trim_end_matches("**").trim_end_matches('/').to_string())
        .unwrap_or_default()
}

/// The node's container, itself included.
fn container_of<'t>(tree: &'t DerivedTree, mut n: &'t Node) -> Option<&'t Node> {
    loop {
        if n.kind == Kind::Container {
            return Some(n);
        }
        n = node(tree, n.parent_id.as_deref()?).ok()?;
    }
}

fn in_subtree(tree: &DerivedTree, root: &str, id: &str) -> bool {
    let mut cur = Some(id);
    while let Some(c) = cur {
        if c == root {
            return true;
        }
        cur = tree.planned.nodes.iter().find(|n| n.id == c).and_then(|n| n.parent_id.as_deref());
    }
    false
}

/// The file most of these claims live in.
fn commonest<'a>(files: impl Iterator<Item = &'a str>) -> Option<String> {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for f in files {
        *counts.entry(f).or_default() += 1;
    }
    counts.into_iter().max_by_key(|&(f, n)| (n, std::cmp::Reverse(f))).map(|(f, _)| f.to_string())
}

fn pascal(name: &str) -> String {
    snake_name(name)
        .split('_')
        .map(|w| w[..1].to_uppercase() + &w[1..])
        .collect()
}

/// The test file a new claim on `node_id` goes into: the one its node's
/// claims already live in, else one of its subtree's, else the language's
/// conventional place for that level.
pub fn stub_target(tree: &DerivedTree, node_id: &str) -> Result<String, String> {
    let n = node(tree, node_id)?;
    let own = tree.claims.iter().filter(|c| c.node_id == node_id).map(|c| c.file.as_str());
    if let Some(f) = commonest(own) {
        return Ok(f);
    }
    let below = tree.claims.iter().filter(|c| in_subtree(tree, node_id, &c.node_id)).map(|c| c.file.as_str());
    if let Some(f) = commonest(below) {
        return Ok(f);
    }
    let container = container_of(tree, n).ok_or("claims at system level need an e2e test file; write the first one by hand")?;
    let dir = container_dir(tree, &container.id);
    let join = |rest: &str| if dir.is_empty() { rest.to_string() } else { format!("{dir}/{rest}") };
    let files: Vec<&str> = tree
        .file_nodes
        .iter()
        .filter(|(_, owner)| in_subtree(tree, node_id, owner))
        .map(|(f, _)| f.as_str())
        .collect();
    let ext = commonest(files.iter().map(|f| f.rsplit('.').next().unwrap_or(""))).unwrap_or_default();
    // .NET tests live in a test project: the one this container's claims use.
    let cs_dir = || {
        tree.claims
            .iter()
            .filter(|c| c.file.ends_with(".cs") && in_subtree(tree, &container.id, &c.node_id))
            .find_map(|c| c.file.rsplit_once('/').map(|(d, _)| d.to_string()))
            .unwrap_or_else(|| format!("tests/{}.Tests", pascal(&container.name)))
    };
    let stem = snake_name(&n.name).replace('_', "-");
    if n.kind == Kind::Container {
        return Ok(match ext.as_str() {
            "rs" => join("tests/claims.rs"),
            "py" => join("tests/test_integration.py"),
            "cs" => format!("{}/{}IntegrationTests.cs", cs_dir(), pascal(&n.name)),
            "tsx" | "ts" | "mts" => join(&format!("tests/{stem}.test.ts")),
            _ => join(&format!("tests/{stem}.test.js")),
        });
    }
    // A component: its root file for Rust's inline tests, a sibling test
    // file for JS, the container's tests/ for Python, the test project for .NET.
    let root = files
        .iter()
        .find(|f| {
            let name = f.rsplit('/').next().unwrap_or(f);
            matches!(name, "mod.rs" | "lib.rs" | "main.rs") || name.split('.').next() == Some(n.name.as_str())
        })
        .or(files.first())
        .ok_or("this component has no files to put a test beside")?;
    let root_dir = root.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
    let beside = |name: &str| if root_dir.is_empty() { name.to_string() } else { format!("{root_dir}/{name}") };
    Ok(match ext.as_str() {
        "rs" => root.to_string(),
        "py" => join(&format!("tests/test_{}.py", snake_name(&n.name))),
        "cs" => format!("{}/{}Tests.cs", cs_dir(), pascal(&n.name)),
        "tsx" | "ts" | "mts" | "cts" => beside(&format!("{}.test.ts", n.name)),
        _ => beside(&format!("{}.test.js", n.name)),
    })
}

/// Reword a derived claim's test description in its file. Returns the file.
pub fn edit_claim_text(project: &Path, resp_id: &str, text: &str) -> Result<String, String> {
    let tree = derived_tree(project)?;
    let claim = tree
        .claims
        .iter()
        .find(|c| c.id == resp_id)
        .ok_or_else(|| format!("no claim {resp_id} in the derived tree"))?;
    let path = project.join(&claim.file);
    let source = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", claim.file))?;
    let site = ClaimSite { identifier: &claim.identifier, line: claim.line, concerns: &claim.concerns };
    let edited = rewrite_description(&claim.file, &source, &site, text)?;
    std::fs::write(&path, edited).map_err(|e| format!("{}: {e}", claim.file))?;
    Ok(claim.file.clone())
}

/// Write `text` as a todo stub for `node_id`. Returns the file written.
pub fn write_claim_stub(project: &Path, node_id: &str, text: &str) -> Result<String, String> {
    let tree = derived_tree(project)?;
    let file = stub_target(&tree, node_id)?;
    let suite = node(&tree, node_id)?.name.clone();
    let path = project.join(&file);
    let source = std::fs::read_to_string(&path).ok();
    let written = insert_stub(&file, source.as_deref(), text, Some(&suite))?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(&path, written).map_err(|e| format!("{file}: {e}"))?;
    Ok(file)
}

/// Set a container's display name in `.scryer/structure.toml`.
pub fn rename_container(project: &Path, node_id: &str, name: &str) -> Result<(), String> {
    let tree = derived_tree(project)?;
    let n = node(&tree, node_id)?;
    if n.kind != Kind::Container {
        return Err("only a container's display name lives in structure.toml".into());
    }
    let dir = container_dir(&tree, node_id);
    let path = project.join(".scryer").join("structure.toml");
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    std::fs::write(&path, set_container_name(&text, &dir, name.trim())).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use scryer_core::derived_tree::ClaimState;

    fn write(dir: &Path, rel: &str, text: &str) {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    /// End to end: a claim reworded through its file reads back reworded, a
    /// stub added to a component lands in its test module and derives as
    /// planned, and a container rename shows on the next derive.
    #[test]
    fn claims_round_trip_through_their_files() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        write(d, "Cargo.toml", "[package]\nname = \"core\"\nversion = \"0.1.0\"\n");
        write(d, "src/lib.rs", "pub mod draft;\npub mod store;\n");
        write(
            d,
            "src/draft.rs",
            "pub fn save() {}\n#[cfg(test)]\nmod tests {\n    /// Saves [io]\n    #[test]\n    fn saves() {}\n}\n",
        );
        write(d, "src/store.rs", "pub fn put() {}\n");
        let tree = derived_tree(d).unwrap();
        let saves = tree.claims.iter().find(|c| c.identifier == "saves").unwrap().id.clone();
        edit_claim_text(d, &saves, "When asked, saves the draft").unwrap();
        let tree = derived_tree(d).unwrap();
        let c = tree.claims.iter().find(|c| c.identifier == "saves").unwrap();
        assert_eq!((c.description.as_str(), c.concerns.clone()), ("When asked, saves the draft", vec!["io".to_string()]));

        let store = tree.planned.nodes.iter().find(|n| n.name == "store").unwrap().id.clone();
        assert_eq!(write_claim_stub(d, &store, "When full, refuses writes").unwrap(), "src/store.rs");
        let tree = derived_tree(d).unwrap();
        let stub = tree.claims.iter().find(|c| c.description == "When full, refuses writes").unwrap();
        assert!(stub.planned);
        assert_eq!(stub.node_id, store);
        let (_, status) = crate::verdict_cache::read_derived_status(
            &scryer_core::ModelRef::ProjectLocal(d.to_path_buf()),
        )
        .unwrap();
        assert_eq!(status.claims.iter().find(|s| s.resp_id == stub.id).unwrap().state, ClaimState::Planned);

        let container = tree.planned.nodes.iter().find(|n| n.kind == Kind::Container).unwrap().id.clone();
        rename_container(d, &container, "Draft Core").unwrap();
        let tree = derived_tree(d).unwrap();
        assert!(tree.planned.nodes.iter().any(|n| n.kind == Kind::Container && n.name == "Draft Core"));
        assert!(rename_container(d, &store, "x").is_err(), "a component's name is its module's");
    }
}
