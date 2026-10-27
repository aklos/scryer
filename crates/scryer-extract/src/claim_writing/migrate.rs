//! One-shot migration from an authored model to the code: each claim with an
//! attached test becomes that test's description (when the test has only
//! its bare name so far), each descriptive statement a module doc on the
//! file it is anchored to, each untested claim a todo stub beside the
//! nearest test file, and the containers' display names and styles go to
//! `structure.toml`. Idempotent: what is already in the code is left alone.

use crate::claim_text::{add_module_doc, insert_stub, rewrite_description, ClaimSite};
use crate::claim_text::{lift_file, sentence_case};
use crate::derivation::set_container_field;
use crate::claim_file_io::stub_target;
use crate::tree_from_disk::derived_tree;
use scryer_core::{Kind, Node, Responsibility, ScryModel, SourceLocation};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use tree_sitter::Parser;

/// What a migration wrote, and what it left and why.
#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct MigrationReport {
    /// `file::test ← statement`
    pub described: Vec<String>,
    /// `file ← statement`
    pub documented: Vec<String>,
    /// `file ← statement`
    pub stubbed: Vec<String>,
    /// `dir: key = value`
    pub structure: Vec<String>,
    /// `statement — reason`
    pub skipped: Vec<String>,
}

impl MigrationReport {
    pub fn wrote_anything(&self) -> bool {
        !(self.described.is_empty() && self.documented.is_empty() && self.stubbed.is_empty() && self.structure.is_empty())
    }
}

fn read_layer(path: &Path) -> Option<ScryModel> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

fn plain(statement: &str) -> String {
    statement.replace("**", "").trim().to_string()
}

/// The one file a boundary glob stands for: the file itself, else the
/// directory's root module, else the file named after the directory, else
/// the directory's only source file.
fn boundary_file(project: &Path, pattern: &str, files: &BTreeSet<String>) -> Option<String> {
    // `dir/**/*`, `dir/*` and `dir/**` all name the directory.
    let pattern = &format!("{}/**", dir_of(pattern));
    let pattern = if pattern == "/**" { "**" } else { pattern.as_str() };
    if !pattern.contains('*') {
        return project.join(pattern).is_file().then(|| pattern.to_string());
    }
    if let Some(f) = root_file(project, pattern) {
        return Some(f);
    }
    let glob = glob::Pattern::new(pattern).ok()?;
    let sources: Vec<&String> = files
        .iter()
        .filter(|f| glob.matches(f) && scryer_core::scan::is_product_code(f) && !crate::derivation::is_test_path(f))
        .collect();
    let dir_name = pattern.trim_end_matches("**").trim_end_matches('/').rsplit('/').next().unwrap_or("");
    sources
        .iter()
        .find(|f| {
            let name = f.rsplit('/').next().unwrap_or(f);
            !dir_name.is_empty() && name.split('.').next() == Some(dir_name)
        })
        .or(if sources.len() == 1 { sources.first() } else { None })
        .map(|f| f.to_string())
}

/// The directory a boundary glob covers.
fn dir_of(pattern: &str) -> &str {
    let mut p = pattern;
    for suffix in ["/**/*", "/**", "/*", "**"] {
        if let Some(rest) = p.strip_suffix(suffix) {
            p = rest;
            break;
        }
    }
    p
}

/// The root file of a directory boundary, where a statement about the whole
/// directory belongs.
fn root_file(project: &Path, pattern: &str) -> Option<String> {
    let dir = dir_of(pattern);
    let join = |f: &str| if dir.is_empty() { f.to_string() } else { format!("{dir}/{f}") };
    let mut candidates: Vec<String> = ["mod.rs", "src/lib.rs", "src/main.rs", "lib.rs", "main.rs", "index.ts", "index.tsx", "src/index.ts", "src/main.tsx", "__init__.py"]
        .iter()
        .map(|f| join(f))
        .collect();
    if !dir.is_empty() {
        candidates.push(format!("{dir}.rs"));
    }
    candidates.into_iter().find(|f| project.join(f).is_file())
}

/// Every node below `id`.
fn descendants<'n>(nodes: &'n BTreeMap<String, Node>, id: &str) -> Vec<&'n Node> {
    let mut out = Vec::new();
    let mut stack = vec![id.to_string()];
    while let Some(cur) = stack.pop() {
        for n in nodes.values().filter(|n| n.parent_id.as_deref() == Some(cur.as_str())) {
            out.push(n);
            stack.push(n.id.clone());
        }
    }
    out
}

fn write(project: &Path, file: &str, text: &str) -> Result<(), String> {
    let path = project.join(file);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, text).map_err(|e| format!("{file}: {e}"))
}

/// Migrate the project's authored model (`.scryer/model.scry`, with
/// `planned.scry` over it) into its code.
pub fn migrate(project: &Path) -> Result<MigrationReport, String> {
    let dir = project.join(".scryer");
    let committed = read_layer(&dir.join("model.scry")).ok_or("no authored model (.scryer/model.scry) to migrate")?;
    let planned = read_layer(&dir.join("planned.scry"));
    let mut report = MigrationReport::default();

    // The working view: the plan's copy of each claim and map entry wins.
    let mut claims: BTreeMap<String, (Node, Responsibility)> = BTreeMap::new();
    let mut test_map: BTreeMap<String, Vec<SourceLocation>> = committed.test_map.clone();
    let mut source_map: BTreeMap<String, Vec<SourceLocation>> = committed.source_map.clone();
    let mut boundaries = committed.boundaries.clone();
    let mut nodes: BTreeMap<String, Node> = BTreeMap::new();
    for layer in std::iter::once(&committed).chain(planned.as_ref()) {
        for n in &layer.nodes {
            nodes.insert(n.id.clone(), n.clone());
            for r in &n.responsibilities {
                claims.insert(r.id.clone(), (n.clone(), r.clone()));
            }
        }
    }
    if let Some(p) = &planned {
        test_map.extend(p.test_map.clone());
        source_map.extend(p.source_map.clone());
        boundaries.extend(p.boundaries.clone());
    }

    let tree = derived_tree(project)?;
    let files = crate::discovery::project_files(project);
    let mut parser = Parser::new();
    let mut described_tests: BTreeSet<(String, String)> = BTreeSet::new();
    let mut docs: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for (id, (host, resp)) in &claims {
        let statement = plain(&resp.statement);
        if statement.is_empty() || host.external == Some(true) || host.kind == Kind::Person {
            continue;
        }
        // 1. A claim with a test: the test's description, if it has none yet.
        if let Some(loc) = test_map.get(id).and_then(|locs| locs.iter().find(|l| l.symbol.is_some())) {
            let file = &loc.pattern;
            let symbol = loc.symbol.as_deref().unwrap_or_default();
            let Ok(source) = std::fs::read_to_string(project.join(file)) else {
                report.skipped.push(format!("{statement} — test file {file} is gone"));
                continue;
            };
            let lifted = lift_file(file, &source, &mut parser);
            let leaf = symbol.rsplit("::").next().unwrap_or(symbol);
            let Some(test) = lifted.tests.iter().find(|t| t.identifier == symbol || t.identifier == leaf) else {
                report.skipped.push(format!("{statement} — no test {symbol} in {file}"));
                continue;
            };
            let key = (file.clone(), test.identifier.clone());
            if test.documented || described_tests.contains(&key) {
                if !described_tests.contains(&key) && test.description != statement {
                    report.skipped.push(format!("{statement} — {file}::{} already says \"{}\"", test.identifier, test.description));
                } else if described_tests.contains(&key) {
                    report.skipped.push(format!("{statement} — shares {file}::{} with another claim", test.identifier));
                }
                continue;
            }
            let site = ClaimSite { identifier: &test.identifier, line: test.line, concerns: &test.concerns };
            let edited = rewrite_description(file, &source, &site, &statement)?;
            write(project, file, &edited)?;
            report.described.push(format!("{file}::{} ← {statement}", test.identifier));
            described_tests.insert(key);
            continue;
        }
        // The claim's own anchor, else its node's declaration.
        let file_of = |key: &str| {
            source_map
                .get(key)
                .and_then(|locs| locs.iter().map(|l| l.pattern.clone()).find(|p| !p.contains('*') && project.join(p).is_file()))
        };
        let anchored = file_of(id).or_else(|| file_of(&host.id));
        // 2. A descriptive statement: the module doc of its file, or of its
        // node's directory root when it is anchored nowhere.
        if !scryer_core::ears::classify(&statement).testable() {
            let target = anchored
                .or_else(|| boundaries.get(&host.id).and_then(|b| b.iter().find_map(|s| boundary_file(project, &s.pattern, &files))))
                .or_else(|| {
                    // A structural node speaks for its descendants' code: their
                    // one file, else the file of the directory they share.
                    let below: BTreeSet<String> = descendants(&nodes, &host.id)
                        .iter()
                        .flat_map(|n| n.responsibilities.iter().map(|r| r.id.clone()).chain(std::iter::once(n.id.clone())))
                        .filter_map(|key| file_of(&key))
                        .collect();
                    match below.len() {
                        0 => None,
                        1 => below.into_iter().next(),
                        _ => {
                            let first = below.iter().next().unwrap();
                            let mut common = first.rsplit_once('/').map_or("", |(d, _)| d).to_string();
                            for f in &below {
                                while !common.is_empty() && !f.starts_with(&format!("{common}/")) {
                                    common = common.rsplit_once('/').map_or(String::new(), |(d, _)| d.to_string());
                                }
                            }
                            boundary_file(project, &format!("{common}/**"), &files)
                        }
                    }
                });
            match target {
                Some(file) => docs.entry(file).or_default().push(statement),
                None => report.skipped.push(format!("{statement} — {} has no file to document", host.name)),
            }
            continue;
        }
        // 3. An untested claim: a todo stub where its code's tests live.
        let node = anchored.as_ref().and_then(|f| tree.file_nodes.get(f));
        let Some(node) = node else {
            report.skipped.push(format!("{statement} — untested and anchored to no code"));
            continue;
        };
        let file = stub_target(&tree, node)?;
        let existing = std::fs::read_to_string(project.join(&file)).ok();
        let already = existing
            .as_deref()
            .is_some_and(|src| lift_file(&file, src, &mut parser).tests.iter().any(|t| t.description == sentence_case(&statement)));
        if already {
            continue;
        }
        let suite = tree.planned.nodes.iter().find(|n| &n.id == node).map(|n| n.name.clone());
        let written = insert_stub(&file, existing.as_deref(), &statement, suite.as_deref())?;
        write(project, &file, &written)?;
        report.stubbed.push(format!("{file} ← {statement}"));
    }

    for (file, statements) in docs {
        let source = std::fs::read_to_string(project.join(&file)).map_err(|e| format!("{file}: {e}"))?;
        match add_module_doc(&file, &source, &statements) {
            Ok(Some(text)) => {
                write(project, &file, &text)?;
                for s in statements.iter().filter(|s| !source.contains(s.as_str())) {
                    report.documented.push(format!("{file} ← {s}"));
                }
            }
            Ok(None) => {}
            Err(e) => {
                for s in statements {
                    report.skipped.push(format!("{s} — {file}: {e}"));
                }
            }
        }
    }

    // 4. Containers' display names and styles, onto the derived container
    // that holds each authored container's directory.
    let structure_path = dir.join("structure.toml");
    let mut structure = std::fs::read_to_string(&structure_path).unwrap_or_default();
    let derived: Vec<(String, &Node)> = tree
        .planned
        .nodes
        .iter()
        .filter(|n| n.kind == Kind::Container)
        .filter_map(|n| {
            let b = tree.planned.boundaries.get(&n.id)?.first()?;
            Some((dir_of(&b.pattern).to_string(), n))
        })
        .collect();
    for n in nodes.values().filter(|n| n.kind == Kind::Container && n.external != Some(true)) {
        let Some(adir) = boundaries.get(&n.id).and_then(|b| b.first()).map(|s| dir_of(&s.pattern).to_string()) else {
            report.skipped.push(format!("container {} — no boundary to locate it", n.name));
            continue;
        };
        let holder = derived
            .iter()
            .filter(|(d, _)| d.is_empty() || adir == *d || adir.starts_with(&format!("{d}/")))
            .max_by_key(|(d, _)| d.len());
        let Some((cdir, current)) = holder else {
            report.skipped.push(format!("container {} — no package holds {adir}", n.name));
            continue;
        };
        if current.name != n.name {
            structure = set_container_field(&structure, cdir, "name", &n.name);
            report.structure.push(format!("{cdir}: name = {}", n.name));
        }
        if let Some(style) = &n.style {
            if current.style.as_deref() != Some(style.as_str()) {
                structure = set_container_field(&structure, cdir, "style", style);
                report.structure.push(format!("{cdir}: style = {style}"));
            }
        }
    }
    if !report.structure.is_empty() {
        write(project, ".scryer/structure.toml", &structure)?;
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put(dir: &Path, rel: &str, text: &str) {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    /// End to end: a tested claim describes its bare test, a descriptive one
    /// documents its file, an untested one stubs a test, a container's name
    /// and style land in structure.toml; a second run writes nothing.
    #[test]
    fn migrates_an_authored_model_into_the_code_once() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        put(d, "Cargo.toml", "[package]\nname = \"core\"\nversion = \"0.1.0\"\n");
        put(d, "src/lib.rs", "pub mod draft;\n");
        put(d, "src/draft.rs", "pub fn save() {}\npub fn load() {}\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn saves() {}\n}\n");
        let model = serde_json::json!({
            "version": "0.3",
            "nodes": [
                { "id": "c", "kind": "container", "name": "Draft Core", "style": "library" },
                { "id": "n", "kind": "component", "name": "Draft", "parentId": "c", "responsibilities": [
                    { "id": "r1", "statement": "**When** asked, **saves** the draft" },
                    { "id": "r2", "statement": "Holds the user's draft." },
                    { "id": "r3", "statement": "**When** reopened, **loads** the draft" }
                ] }
            ],
            "links": [],
            "testMap": { "r1": [{ "pattern": "src/draft.rs", "symbol": "saves" }] },
            "sourceMap": {
                "r2": [{ "pattern": "src/draft.rs" }],
                "r3": [{ "pattern": "src/draft.rs", "symbol": "load" }]
            },
            "boundaries": { "c": [{ "pattern": "**" }] }
        });
        put(d, ".scryer/model.scry", &model.to_string());

        let r = migrate(d).unwrap();
        assert_eq!(r.described, vec!["src/draft.rs::saves ← When asked, saves the draft"]);
        assert_eq!(r.documented, vec!["src/draft.rs ← Holds the user's draft."]);
        assert_eq!(r.stubbed, vec!["src/draft.rs ← When reopened, loads the draft"]);
        assert_eq!(r.structure, vec![": name = Draft Core", ": style = library"]);

        let tree = derived_tree(d).unwrap();
        let by_text = |t: &str| tree.claims.iter().find(|c| c.description == t).unwrap();
        assert!(!by_text("When asked, saves the draft").planned);
        assert!(by_text("When reopened, loads the draft").planned);
        assert!(tree.planned.nodes.iter().any(|n| n.description.as_deref() == Some("Holds the user's draft.")));
        assert!(tree.planned.nodes.iter().any(|n| n.kind == Kind::Container && n.name == "Draft Core"));

        let again = migrate(d).unwrap();
        assert!(!again.wrote_anything(), "{again:?}");
    }
}
