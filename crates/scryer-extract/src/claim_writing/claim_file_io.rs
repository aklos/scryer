//! Claims written back to disk: a reworded description into its test file,
//! a new claim as a todo stub into the test file its node's tests live in,
//! a node's readable name into `structure.toml`, a node's description where
//! it is read from and its decisions into its module doc.

use crate::claim_text::{
    insert_stub, lift_file, rewrite_description, set_module_body, set_module_summary, snake_name, split_concerns, ClaimSite,
};
use crate::lang;
use crate::derivation::{set_section_field, Structure, ARCHITECTURE_FILE, STRUCTURE_FILE};
use crate::tree_from_disk::{derived_tree, read_structure};
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

/// Set a node's readable name in `.scryer/structure.toml`: the system's
/// under `[system]`, a person's, external's, container's or component's
/// under the entry keyed by what the code or `[system]` calls it. Only the
/// name changes; whatever refers to the node does so by its key. A symbol is
/// named by its code.
pub fn rename_node(project: &Path, node_id: &str, name: &str) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("a name can't be empty".into());
    }
    let tree = derived_tree(project)?;
    let n = node(&tree, node_id)?;
    let s = read_structure(project)?;
    let text = std::fs::read_to_string(project.join(STRUCTURE_FILE)).unwrap_or_default();
    let (header, key) = structure_entry(&tree, &s, n)?;
    let text = match n.kind {
        Kind::Container => refer_by_key(&text, &s, &n.name, &key)?,
        _ => text,
    };
    write_project_file(project, STRUCTURE_FILE, &set_section_field(&text, &header, "name", name))
}

/// The `structure.toml` section that configures a node, as the headers it
/// may be written under, and the node's key there: `[system]`, a person's
/// or external's key under `[system]`, a container's directory or package
/// name, a component's path. A symbol has none.
fn structure_entry(tree: &DerivedTree, s: &Structure, n: &Node) -> Result<(Vec<String>, String), String> {
    let table = |table: &str, key: &str| vec![format!("[{table}.\"{key}\"]"), format!("[{table}.{key}]")];
    match n.kind {
        Kind::System if n.external.is_none() => Ok((vec!["[system]".to_string()], String::new())),
        Kind::Person | Kind::System => {
            let (name, keys) =
                if n.kind == Kind::Person { ("persons", &s.system.persons) } else { ("externals", &s.system.externals) };
            let key = keys
                .iter()
                .find(|k| s.context_name(k).unwrap_or(k) == n.name)
                .ok_or_else(|| format!("{} is not under [system] in structure.toml", n.name))?;
            Ok((table(name, key), key.clone()))
        }
        Kind::Container => {
            // The entry already configuring it, by directory, display name
            // or package name; else a new one keyed by directory.
            let dir = container_dir(tree, &n.id);
            let key = s
                .containers
                .iter()
                .find(|(k, _)| **k == dir)
                .or_else(|| s.containers.iter().find(|(_, c)| c.name.as_deref() == Some(n.name.as_str())))
                .or_else(|| s.containers.iter().find(|(k, _)| **k == n.name))
                .map_or(dir, |(k, _)| k.clone());
            Ok((table("containers", &key), key))
        }
        Kind::Component => {
            let key = tree
                .planned
                .boundaries
                .get(&n.id)
                .and_then(|b| b.first())
                .map(|src| src.pattern.strip_suffix("/**").unwrap_or(&src.pattern).to_string())
                .ok_or_else(|| format!("{} has no path to key it by", n.name))?;
            Ok((table("components", &key), key))
        }
        Kind::Symbol => Err("a symbol is named and documented by its code; edit it there".into()),
    }
}

fn write_project_file(project: &Path, rel: &str, text: &str) -> Result<(), String> {
    let path = project.join(rel);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(&path, text).map_err(|e| format!("{rel}: {e}"))
}

/// `[relationships]` with every end naming a container by its display name
/// `old` named by its key instead, so renaming it breaks nothing. Lines of
/// other sections are left as written. A key that is also the system's,
/// a person's or an external's name would point the relationship there, so
/// the rename is refused instead.
fn refer_by_key(text: &str, s: &crate::derivation::Structure, old: &str, key: &str) -> Result<String, String> {
    if old == key || !s.relationships().any(|(a, b, _)| a == old || b == old) {
        return Ok(text.to_string());
    }
    let sys = &s.system;
    if sys.name.as_deref() == Some(key) || sys.persons.iter().chain(&sys.externals).any(|k| k == key) {
        return Err(format!(
            "[relationships] names \"{old}\", and its key \"{key}\" also names something under [system]; key the container by its directory in structure.toml first"
        ));
    }
    let (from, to) = (format!("\"{old}\""), format!("\"{key}\""));
    let mut in_relationships = false;
    let mut out: Vec<String> = Vec::new();
    for line in text.lines() {
        let header = line.trim();
        if header.starts_with('[') && header.ends_with(']') && !header.contains('=') {
            in_relationships = header == "[relationships]";
        }
        out.push(if in_relationships { line.replace(&from, &to) } else { line.to_string() });
    }
    let mut joined = out.join("\n");
    if text.ends_with('\n') {
        joined.push('\n');
    }
    Ok(joined)
}

/// Set a node's description where it is read from: a module doc's first
/// paragraph (a layer the doc declared stays declared), a manifest's
/// `description`, `ARCHITECTURE.md`'s first paragraph for the system, or
/// `structure.toml` for a node no code documents. Returns the file.
pub fn edit_node_description(project: &Path, node_id: &str, text: &str) -> Result<String, String> {
    let tree = derived_tree(project)?;
    let n = node(&tree, node_id)?;
    let file = tree
        .doc_files
        .get(node_id)
        .ok_or("a symbol's description is its doc comment; edit it in the code")?
        .clone();
    let summary = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let source = std::fs::read_to_string(project.join(&file)).unwrap_or_default();
    let edited = match file.rsplit('/').next().unwrap_or(&file) {
        ARCHITECTURE_FILE => set_markdown_part(&source, Some(&summary), None)?,
        "structure.toml" => {
            let (header, _) = structure_entry(&tree, &read_structure(project)?, n)?;
            set_section_field(&source, &header, "description", &summary)
        }
        "Cargo.toml" => set_section_field(&source, &["[package]".to_string()], "description", &summary),
        "pyproject.toml" => set_section_field(&source, &["[project]".to_string()], "description", &summary),
        "package.json" => set_json_description(&source, &summary)?,
        _ => {
            let old = lift_file(&file, &source, &mut tree_sitter::Parser::new()).module_description.unwrap_or_default();
            let mut summary = summary;
            if let Some(layer) = &n.layer {
                if split_concerns(&old).1 == [layer.clone()] && split_concerns(&summary).1.is_empty() {
                    summary.push_str(&format!(" [{layer}]"));
                }
            }
            set_module_summary(&file, &source, &summary)?
        }
    };
    write_project_file(project, &file, &edited)?;
    Ok(file)
}

/// Set a node's decisions: its module doc after the first paragraph, or
/// `ARCHITECTURE.md` after its first for the system. Returns the file.
pub fn edit_node_decisions(project: &Path, node_id: &str, text: &str) -> Result<String, String> {
    let tree = derived_tree(project)?;
    node(&tree, node_id)?;
    let file = tree.doc_files.get(node_id).cloned().unwrap_or_default();
    let code = lang::language_for_ext(lang::ext_of(Path::new(&file)).unwrap_or_default()).is_some();
    let source = std::fs::read_to_string(project.join(&file)).unwrap_or_default();
    let edited = if file == ARCHITECTURE_FILE {
        set_markdown_part(&source, None, Some(text.trim()))?
    } else if code {
        set_module_body(&file, &source, text)?
    } else {
        return Err("decisions live in a module doc; this node has none to hold them".into());
    };
    write_project_file(project, &file, &edited)?;
    Ok(file)
}

/// A markdown file with its first paragraph (under any title heading) or
/// the rest after it replaced. A file with no first paragraph takes the
/// summary first; decisions need one to follow.
fn set_markdown_part(text: &str, summary: Option<&str>, body: Option<&str>) -> Result<String, String> {
    let lines: Vec<&str> = text.lines().collect();
    let start = lines
        .iter()
        .position(|l| !l.trim().is_empty() && !l.trim_start().starts_with("# "))
        .unwrap_or(lines.len());
    let end = lines[start..].iter().position(|l| l.trim().is_empty()).map_or(lines.len(), |n| start + n);
    if start == end && summary.is_none() {
        return Err("write the description first: decisions follow it".into());
    }
    let mut out: Vec<String> = lines[..start].iter().map(|l| l.to_string()).collect();
    if start > 0 && !out.last().is_some_and(|l| l.trim().is_empty()) {
        out.push(String::new());
    }
    match summary {
        Some(s) if s.is_empty() => return Err("a description can't be empty".into()),
        Some(s) => out.push(s.to_string()),
        None => out.extend(lines[start..end].iter().map(|l| l.to_string())),
    }
    let rest = match body {
        Some(b) => b.to_string(),
        None => lines[end..].join("\n").trim().to_string(),
    };
    if !rest.is_empty() {
        out.push(String::new());
        out.push(rest);
    }
    Ok(out.join("\n") + "\n")
}

/// A package.json with its `description` set, the rest of the text as
/// written.
fn set_json_description(text: &str, summary: &str) -> Result<String, String> {
    let quoted = serde_json::to_string(summary).map_err(|e| e.to_string())?;
    // The value of the first `"description":`, a JSON string, by its quotes.
    if let Some(at) = text.find("\"description\"") {
        let after = &text[at + "\"description\"".len()..];
        let colon = after.len() - after.trim_start().len();
        if let Some(value) = after.trim_start().strip_prefix(':') {
            let ws = value.len() - value.trim_start().len();
            let from = at + "\"description\"".len() + colon + 1 + ws;
            if text[from..].starts_with('"') {
                let mut escaped = false;
                let close = text[from + 1..].char_indices().find(|&(_, c)| {
                    let end = c == '"' && !escaped;
                    escaped = c == '\\' && !escaped;
                    end
                });
                if let Some((i, _)) = close {
                    return Ok(format!("{}{quoted}{}", &text[..from], &text[from + 1 + i + 1..]));
                }
            }
        }
    }
    let open = text.find('{').ok_or("package.json is not an object")?;
    Ok(format!("{}\n  \"description\": {quoted},{}", &text[..=open], &text[open + 1..]))
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
        rename_node(d, &container, "Draft Core").unwrap();
        let tree = derived_tree(d).unwrap();
        assert!(tree.planned.nodes.iter().any(|n| n.kind == Kind::Container && n.name == "Draft Core"));
    }

    /// When a description or decisions are edited, they land where they are read from (a module doc, a manifest, ARCHITECTURE.md, structure.toml) and read back on the next derive.
    #[test]
    fn descriptions_and_decisions_land_where_they_are_read() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        write(d, "Cargo.toml", "[package]\nname = \"core\"\nversion = \"0.1.0\"\ndescription = \"Old.\"\n");
        write(d, "src/lib.rs", "pub mod store;\n");
        write(d, "src/store.rs", "//! Stores drafts.\npub fn put() {}\n");
        write(d, ".scryer/structure.toml", "[system]\npersons = [\"dev\"]\n");
        let id = |name: &str| derived_tree(d).unwrap().planned.nodes.iter().find(|n| n.name == name).unwrap().id.clone();
        let system = derived_tree(d).unwrap().planned.nodes.iter().find(|n| n.kind == Kind::System).unwrap().id.clone();
        assert!(edit_node_decisions(d, &system, "One store.").is_err(), "decisions follow a description");
        assert_eq!(edit_node_description(d, &system, "Keeps drafts safe.").unwrap(), "ARCHITECTURE.md");
        edit_node_decisions(d, &system, "One store, no sync.").unwrap();
        assert_eq!(edit_node_description(d, &id("core"), "The draft core.").unwrap(), "Cargo.toml");
        edit_node_description(d, &id("dev"), "Writes drafts.").unwrap();
        assert_eq!(edit_node_decisions(d, &id("store"), "Append-only.").unwrap(), "src/store.rs");
        let tree = derived_tree(d).unwrap();
        let by = |name: &str| tree.planned.nodes.iter().find(|n| n.name == name).unwrap();
        let sys = tree.planned.nodes.iter().find(|n| n.kind == Kind::System).unwrap();
        assert_eq!((sys.description.as_deref(), sys.decisions.as_deref()), (Some("Keeps drafts safe."), Some("One store, no sync.")));
        assert_eq!(by("core").description.as_deref(), Some("The draft core."));
        assert_eq!(by("dev").description.as_deref(), Some("Writes drafts."));
        assert_eq!((by("store").description.as_deref(), by("store").decisions.as_deref()), (Some("Stores drafts."), Some("Append-only.")));
        assert!(edit_node_decisions(d, &id("dev"), "x").is_err(), "a person has no module doc for decisions");
        let json = set_json_description("{\n  \"name\": \"web\",\n  \"description\": \"Old \\\"one\\\"\"\n}\n", "New.").unwrap();
        assert_eq!(json, "{\n  \"name\": \"web\",\n  \"description\": \"New.\"\n}\n");
    }

    /// When the system, a person, an external, a container or a component is
    /// renamed, its readable name reads back on the next derive, and the
    /// relationships between them still hold.
    #[test]
    fn renames_every_node_above_code() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        write(d, "Cargo.toml", "[package]\nname = \"core\"\nversion = \"0.1.0\"\n");
        write(d, "src/lib.rs", "pub mod drafts;\npub mod store;\n");
        write(d, "src/drafts/mod.rs", "pub fn save() {}\n");
        write(d, "src/store.rs", "pub fn put() {}\n");
        write(
            d,
            ".scryer/structure.toml",
            "[system]\nname = \"app\"\npersons = [\"dev\"]\nexternals = [\"git\"]\n\n[containers.core]\nname = \"Core\"\n\n[relationships]\ndev = [{ to = \"Core\", label = \"edits\" }]\n\"Core\" = [\"git\"]\n",
        );
        let id = |name: &str| derived_tree(d).unwrap().planned.nodes.iter().find(|n| n.name == name).unwrap().id.clone();
        let links = || derived_tree(d).unwrap().planned.links.len();
        let before = links();
        for (old, new) in [
            ("app", "The App"),
            ("dev", "Developer"),
            ("git", "Git"),
            ("Core", "Draft Core"),
            ("drafts", "Drafts"),
            ("store", "Storage"),
        ] {
            rename_node(d, &id(old), new).unwrap();
            assert!(derived_tree(d).unwrap().planned.nodes.iter().any(|n| n.name == new), "{old} → {new}");
        }
        assert_eq!(links(), before, "every relationship still resolves");
        let toml = std::fs::read_to_string(d.join(".scryer/structure.toml")).unwrap();
        assert!(toml.contains("[components.\"src/drafts\"]\nname = \"Drafts\""), "{toml}");
        assert!(toml.contains("[components.\"src/store.rs\"]\nname = \"Storage\""), "{toml}");
        assert!(toml.contains("[persons.dev]") || toml.contains("[persons.\"dev\"]"), "{toml}");
        assert!(!toml.contains("\"Core\" = "), "the relationship refers to the container by its key: {toml}");
        let save = derived_tree(d).unwrap().planned.nodes.iter().find(|n| n.kind == Kind::Symbol).unwrap().id.clone();
        assert!(rename_node(d, &save, "x").is_err(), "a symbol is named by its code");
    }

    /// When a container's or component's description is edited, it lands in
    /// its module doc and reads back on the next derive, a declared layer
    /// kept declared.
    #[test]
    fn descriptions_round_trip_through_module_docs() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        write(d, "Cargo.toml", "[package]\nname = \"core\"\nversion = \"0.1.0\"\n");
        write(d, "src/lib.rs", "//! The core.\npub mod draft;\npub mod store;\n");
        write(d, "src/draft.rs", "//! Holds drafts. [domain]\npub fn save() {}\n");
        write(d, "src/store.rs", "pub fn put() {}\n");
        write(d, ".scryer/structure.toml", "[containers.core]\nlayers = [\"domain\", \"adapters\"]\n");
        let tree = derived_tree(d).unwrap();
        let id = |tree: &DerivedTree, name: &str| tree.planned.nodes.iter().find(|n| n.name == name).unwrap().id.clone();
        let (draft, store) = (id(&tree, "draft"), id(&tree, "store"));
        let container = tree.planned.nodes.iter().find(|n| n.kind == Kind::Container).unwrap().id.clone();

        assert_eq!(edit_node_description(d, &draft, "Keeps the user's drafts.").unwrap(), "src/draft.rs");
        assert_eq!(edit_node_description(d, &store, "Stores things.").unwrap(), "src/store.rs");
        assert_eq!(edit_node_description(d, &container, "The draft core.").unwrap(), "src/lib.rs");
        let tree = derived_tree(d).unwrap();
        let get = |id: &str| tree.planned.nodes.iter().find(|n| n.id == id).unwrap();
        assert_eq!(get(&draft).description.as_deref(), Some("Keeps the user's drafts."));
        assert_eq!(get(&draft).layer.as_deref(), Some("domain"));
        assert_eq!(get(&store).description.as_deref(), Some("Stores things."));
        assert_eq!(get(&container).description.as_deref(), Some("The draft core."));
    }
}
