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
    /// The checklist this run left for review, when it wrote one.
    pub checklist: Option<String>,
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

/// A test name with at least this many content words says enough to judge
/// a claim against.
const DESCRIPTIVE_NAME_WORDS: usize = 3;
/// The least share of a descriptive name's words a claim must use.
const MIN_NAME_FIT: f64 = 0.3;

/// The content words of a test name or claim, cut to their first four
/// letters so `trips` meets `tripped`: split on `_` and camelCase, minus
/// short words and function words.
fn name_words(text: &str) -> BTreeSet<String> {
    const FUNCTION_WORDS: &[&str] = &[
        "an", "the", "of", "to", "in", "on", "at", "by", "for", "with", "and", "or", "its", "it", "is", "are", "be", "as",
        "from", "into", "onto", "out", "off", "that", "this", "then", "when", "while", "if", "no", "not", "than", "every",
        "each", "all", "any", "one", "two", "only", "own", "their", "them", "they", "which", "what", "who",
    ];
    let mut spaced = String::with_capacity(text.len());
    let mut prev_lower = false;
    for ch in text.chars() {
        if ch.is_uppercase() && prev_lower {
            spaced.push(' ');
        }
        prev_lower = ch.is_lowercase();
        spaced.push(ch);
    }
    spaced
        .to_lowercase()
        .split(|c: char| !c.is_ascii_alphabetic())
        .filter(|w| w.len() > 1 && !FUNCTION_WORDS.contains(w))
        .map(|w| w.chars().take(4).collect())
        .collect()
}

/// The share of a test name's content words that a claim also uses.
fn name_fit(statement: &str, identifier: &str) -> f64 {
    let name = name_words(identifier);
    if name.is_empty() {
        return 0.0;
    }
    let claim = name_words(statement);
    name.intersection(&claim).count() as f64 / name.len() as f64
}

fn has_csproj(dir: &Path) -> bool {
    std::fs::read_dir(dir)
        .map(|entries| entries.flatten().any(|e| e.path().extension().is_some_and(|x| x == "csproj")))
        .unwrap_or(false)
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
    let mut by_test: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
    let mut docs: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for (id, (host, resp)) in &claims {
        let statement = plain(&resp.statement);
        if statement.is_empty() || host.external == Some(true) || host.kind == Kind::Person {
            continue;
        }
        // 1. A claim with a test: gathered per test, written after the loop.
        if let Some(loc) = test_map.get(id).and_then(|locs| locs.iter().find(|l| l.symbol.is_some())) {
            let key = (loc.pattern.clone(), loc.symbol.clone().unwrap_or_default());
            by_test.entry(key).or_default().push(statement);
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
        // A .NET test outside a test project never builds.
        let stub_dir = file.rsplit_once('/').map_or("", |(d, _)| d);
        if file.ends_with(".cs") && !has_csproj(&project.join(stub_dir)) {
            report.skipped.push(format!("{statement} — no test project at {stub_dir} for its todo test"));
            continue;
        }
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

    // 1. Each test takes the one claim closest to its name, and only when
    // the claim fits a name descriptive enough to judge it by: an old model
    // often linked a claim to a related test rather than the one checking it.
    for ((file, symbol), statements) in by_test {
        let Ok(source) = std::fs::read_to_string(project.join(&file)) else {
            report.skipped.extend(statements.iter().map(|s| format!("{s} — test file {file} is gone")));
            continue;
        };
        let lifted = lift_file(&file, &source, &mut parser);
        let leaf = symbol.rsplit("::").next().unwrap_or(&symbol);
        let Some(test) = lifted.tests.iter().find(|t| t.identifier == symbol || t.identifier == leaf) else {
            report.skipped.extend(statements.iter().map(|s| format!("{s} — no test {symbol} in {file}")));
            continue;
        };
        let at = format!("{file}::{}", test.identifier);
        if test.documented {
            for s in statements.iter().filter(|s| **s != test.description) {
                report.skipped.push(format!("{s} — {at} already says \"{}\"", test.description));
            }
            continue;
        }
        let (best, fit) = statements
            .iter()
            .map(|s| (s, name_fit(s, &test.identifier)))
            .fold(None::<(&String, f64)>, |acc, (s, f)| match acc {
                Some((_, g)) if g >= f => acc,
                _ => Some((s, f)),
            })
            .expect("a test is gathered with at least one claim");
        for s in statements.iter().filter(|s| *s != best) {
            report.skipped.push(format!("{s} — shares {at} with a claim closer to its name"));
        }
        if name_words(&test.identifier).len() >= DESCRIPTIVE_NAME_WORDS && fit < MIN_NAME_FIT {
            report.skipped.push(format!("{best} — {at} names something else"));
            continue;
        }
        let site = ClaimSite { identifier: &test.identifier, line: test.line, concerns: &test.concerns };
        let edited = rewrite_description(&file, &source, &site, best)?;
        write(project, &file, &edited)?;
        report.described.push(format!("{at} ← {best}"));
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
            let inside: Vec<&str> =
                derived.iter().filter(|(d, _)| d.starts_with(&format!("{adir}/"))).map(|(d, _)| d.as_str()).collect();
            if inside.is_empty() {
                report.skipped.push(format!("container {} — no package holds {adir}", n.name));
            } else {
                let style = n.style.as_deref().map(|s| format!(" (style {s})")).unwrap_or_default();
                report.skipped.push(format!(
                    "container {}{style} — {adir} is now {}; set each one's style in structure.toml by hand",
                    n.name,
                    inside.join(", ")
                ));
            }
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
    // What was decided by rule is still to be judged: the review an agent
    // finishes with the developer. One already there may be half ticked.
    let has_items = report.wrote_anything() || !report.skipped.is_empty();
    if has_items && !project.join(CHECKLIST).exists() {
        write(project, CHECKLIST, &checklist(&report))?;
        report.checklist = Some(CHECKLIST.to_string());
    }
    Ok(report)
}

/// Where a migration leaves its review checklist.
pub const CHECKLIST: &str = ".scryer/migration.md";

/// How many of the migration checklist's items are still open, when there
/// is a checklist.
pub fn open_items(project: &Path) -> Option<usize> {
    let text = std::fs::read_to_string(project.join(CHECKLIST)).ok()?;
    Some(text.lines().filter(|l| l.trim_start().starts_with("- [ ]")).count())
}

/// The review a mechanical migration leaves: every write to confirm and
/// every claim it could not place, each a box to tick.
fn checklist(r: &MigrationReport) -> String {
    let mut out = String::from(
        "# Migration checklist\n\n\
         `scryer-mcp migrate` moved the old authored model into the code by rule. Each item needs a judgment \
         against today's code; tick it once settled. The old model can be weeks behind the code, so a claim \
         that is no longer true is deleted, not built. Delete this file when no item is left.\n",
    );
    let mut section = |title: &str, guide: &str, items: &[String]| {
        if items.is_empty() {
            return;
        }
        out.push_str(&format!("\n## {title} ({})\n\n{guide}\n\n", items.len()));
        let mut sorted: Vec<&String> = items.iter().collect();
        sorted.sort();
        for item in sorted {
            out.push_str(&format!("- [ ] {item}\n"));
        }
    };
    section(
        "Written as test descriptions",
        "Confirm the test checks the claim and that it is still true. Otherwise reword it, move it to the test \
         that does check it, or remove the summary so the test's name speaks.",
        &r.described,
    );
    section(
        "Written as todo tests",
        "Keep the ones still wanted as work to do; delete the rest.",
        &r.stubbed,
    );
    section("Written as module docs", "Confirm each still describes its file.", &r.documented);
    section("Written to structure.toml", "Confirm each style fits the code as it is.", &r.structure);
    section(
        "Not written",
        "For each: describe the test that checks it, write a todo test where it belongs, or drop it when it \
         is no longer true. The reason after the dash says why the rule left it.",
        &r.skipped,
    );
    out
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

    /// A one-crate project with `src/draft.rs` holding `tests`, and an
    /// authored model of one component whose claims map to them.
    fn rust_project(d: &Path, tests: &[&str], claims: serde_json::Value, test_map: serde_json::Value) {
        put(d, "Cargo.toml", "[package]\nname = \"core\"\nversion = \"0.1.0\"\n");
        put(d, "src/lib.rs", "pub mod draft;\n");
        let fns: String = tests.iter().map(|t| format!("    #[test]\n    fn {t}() {{}}\n")).collect();
        put(d, "src/draft.rs", &format!("pub fn save() {{}}\n#[cfg(test)]\nmod tests {{\n{fns}}}\n"));
        let model = serde_json::json!({
            "version": "0.3",
            "nodes": [
                { "id": "c", "kind": "container", "name": "core" },
                { "id": "n", "kind": "component", "name": "draft", "parentId": "c", "responsibilities": claims }
            ],
            "links": [],
            "testMap": test_map,
            "boundaries": { "c": [{ "pattern": "**" }] }
        });
        put(d, ".scryer/model.scry", &model.to_string());
    }

    /// When a migration runs, writes what it wrote and left as a checklist
    /// in `.scryer/migration.md`, never over one already there.
    #[test]
    fn leaves_a_checklist_of_what_it_wrote_and_left() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        rust_project(
            d,
            &["saves"],
            serde_json::json!([
                { "id": "r1", "statement": "When asked, writes the draft" },
                { "id": "r2", "statement": "When the window resizes, redraws the map" }
            ]),
            serde_json::json!({
                "r1": [{ "pattern": "src/draft.rs", "symbol": "saves" }],
                "r2": [{ "pattern": "src/draft.rs", "symbol": "gone" }]
            }),
        );
        let r = migrate(d).unwrap();
        assert_eq!(r.checklist.as_deref(), Some(CHECKLIST));
        let text = std::fs::read_to_string(d.join(CHECKLIST)).unwrap();
        assert!(text.contains("## Written as test descriptions (1)"), "{text}");
        assert!(text.contains("- [ ] src/draft.rs::saves ← When asked, writes the draft"), "{text}");
        assert!(text.contains("## Not written (1)"), "{text}");
        assert!(text.contains("- [ ] When the window resizes, redraws the map — no test gone in src/draft.rs"), "{text}");
        assert_eq!(open_items(d), Some(2));

        let ticked = text.replacen("- [ ]", "- [x]", 1);
        std::fs::write(d.join(CHECKLIST), &ticked).unwrap();
        let again = migrate(d).unwrap();
        assert_eq!(again.checklist, None);
        assert_eq!(std::fs::read_to_string(d.join(CHECKLIST)).unwrap(), ticked, "a half-ticked checklist is kept");
        assert_eq!(open_items(d), Some(1));
    }

    /// When several claims name one test, writes the one closest to the
    /// test's name and reports the others.
    #[test]
    fn writes_the_claim_closest_to_its_tests_name() {
        let dir = tempfile::tempdir().unwrap();
        let at = serde_json::json!([{ "pattern": "src/draft.rs", "symbol": "rejects_an_empty_title_on_save" }]);
        rust_project(
            dir.path(),
            &["rejects_an_empty_title_on_save"],
            serde_json::json!([
                { "id": "r1", "statement": "When a draft is saved, writes it to disk" },
                { "id": "r2", "statement": "If the title is empty, then rejects the save" }
            ]),
            serde_json::json!({ "r1": at, "r2": at }),
        );
        let r = migrate(dir.path()).unwrap();
        assert_eq!(r.described, vec!["src/draft.rs::rejects_an_empty_title_on_save ← If the title is empty, then rejects the save"]);
        assert!(r.skipped.iter().any(|s| s.starts_with("When a draft is saved") && s.ends_with("with a claim closer to its name")), "{r:?}");
    }

    /// If a claim shares too few words with its test's descriptive name, then
    /// leaves the test as it is and reports the mismatch.
    #[test]
    fn leaves_a_test_whose_name_says_something_else() {
        let dir = tempfile::tempdir().unwrap();
        rust_project(
            dir.path(),
            &["rejects_an_empty_title_on_save", "saves"],
            serde_json::json!([
                { "id": "r1", "statement": "When the window resizes, redraws the map" },
                { "id": "r2", "statement": "When asked, writes the draft" }
            ]),
            serde_json::json!({
                "r1": [{ "pattern": "src/draft.rs", "symbol": "rejects_an_empty_title_on_save" }],
                "r2": [{ "pattern": "src/draft.rs", "symbol": "saves" }]
            }),
        );
        let before = std::fs::read_to_string(dir.path().join("src/draft.rs")).unwrap();
        let r = migrate(dir.path()).unwrap();
        assert_eq!(r.described, vec!["src/draft.rs::saves ← When asked, writes the draft"], "a short name is no evidence against a claim");
        assert!(r.skipped.iter().any(|s| s.ends_with("rejects_an_empty_title_on_save names something else")), "{r:?}");
        let after = std::fs::read_to_string(dir.path().join("src/draft.rs")).unwrap();
        assert!(!after.contains("redraws the map") && after.len() > before.len(), "{after}");
    }

    /// A .NET solution: `src/Game` with one view, and the test project
    /// `tests/Client.Tests` only when `with_tests`.
    fn dotnet_project(d: &Path, with_tests: bool) {
        put(d, "src/Game/Game.csproj", "<Project Sdk=\"Microsoft.NET.Sdk\"></Project>\n");
        put(d, "src/Game/View.cs", "namespace Game;\npublic class View { public void Draw() {} }\n");
        if with_tests {
            put(d, "tests/Game.Tests/Game.Tests.csproj", "<Project Sdk=\"Microsoft.NET.Sdk\"></Project>\n");
        }
        let model = serde_json::json!({
            "version": "0.3",
            "nodes": [
                { "id": "c", "kind": "container", "name": "Game" },
                { "id": "n", "kind": "component", "name": "View", "parentId": "c", "responsibilities": [
                    { "id": "r1", "statement": "When the pilot double-clicks an item, takes it onto the racks" }
                ] }
            ],
            "links": [],
            "sourceMap": { "r1": [{ "pattern": "src/Game/View.cs", "symbol": "Draw" }] },
            "boundaries": { "c": [{ "pattern": "src/Game/**" }] }
        });
        put(d, ".scryer/model.scry", &model.to_string());
    }

    /// If an untested .NET claim's todo test would go where there is no test
    /// project, then leaves it out and reports it.
    #[test]
    fn never_stubs_outside_a_test_project() {
        let dir = tempfile::tempdir().unwrap();
        dotnet_project(dir.path(), false);
        let r = migrate(dir.path()).unwrap();
        assert!(r.stubbed.is_empty(), "{r:?}");
        assert!(r.skipped.iter().any(|s| s.contains("no test project at")), "{r:?}");
        assert!(!dir.path().join("tests").exists());

        let dir = tempfile::tempdir().unwrap();
        dotnet_project(dir.path(), true);
        let r = migrate(dir.path()).unwrap();
        assert_eq!(r.stubbed.len(), 1, "{r:?}");
    }

    /// When an authored container's directory holds several packages, names
    /// them in the report for its style to be set by hand.
    #[test]
    fn names_the_packages_a_split_container_became() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        put(d, "src/Client/Core/Core.csproj", "<Project Sdk=\"Microsoft.NET.Sdk\"></Project>\n");
        put(d, "src/Client/Core/Feed.cs", "namespace Core;\npublic class Feed {}\n");
        put(d, "src/Client/Game/Game.csproj", "<Project Sdk=\"Microsoft.NET.Sdk\"></Project>\n");
        put(d, "src/Client/Game/Main.cs", "namespace Game;\npublic class Main {}\n");
        let model = serde_json::json!({
            "version": "0.3",
            "nodes": [{ "id": "c", "kind": "container", "name": "Client", "style": "mvvm" }],
            "links": [],
            "boundaries": { "c": [{ "pattern": "src/Client/**" }] }
        });
        put(d, ".scryer/model.scry", &model.to_string());
        let r = migrate(d).unwrap();
        assert!(
            r.skipped.iter().any(|s| s.starts_with("container Client (style mvvm) — src/Client is now src/Client/Core, src/Client/Game")),
            "{r:?}"
        );
    }
}
