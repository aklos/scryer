//! What the MCP reads: the C4 tree derived from packaging and tests, never the
//! authored model. Claims are behaviour tests; a file maps to the component
//! whose boundary holds it; status comes from the test, not from a tool call.

use scryer_core::derived_tree::{
    ClaimLevel, ClaimState, ClaimStatus, DerivedClaim, DerivedStatus, DerivedTree, ProbeMark,
};
use scryer_core::{Kind, ModelRef, Node, ScryModel};
use std::collections::{BTreeSet, HashMap, HashSet};

/// The derived tree with each claim's status.
pub(crate) struct Loaded {
    pub tree: DerivedTree,
    pub status: DerivedStatus,
    /// The style table the tree was checked against: the built-ins plus the
    /// project's own, for the rule a file's layer is held to.
    pub styles: scryer_core::style::Styles,
    /// The project's comment threads, open ones first.
    pub threads: Vec<scryer_core::comments::Thread>,
}

impl Loaded {
    /// The open threads on `node` and on every node above it: what people
    /// and other agents asked of the code this file belongs to.
    pub(crate) fn open_threads_on(&self, node: &str) -> Vec<&scryer_core::comments::Thread> {
        let model = &self.tree.planned;
        let mut chain = Vec::new();
        let mut cur = Some(node.to_string());
        while let Some(id) = cur {
            if chain.contains(&id) || chain.len() > 64 {
                break;
            }
            cur = model.nodes.iter().find(|n| n.id == id).and_then(|n| n.parent_id.clone());
            chain.push(id);
        }
        self.threads
            .iter()
            .filter(|t| t.status == scryer_core::comments::ThreadStatus::Open && chain.contains(&t.node))
            .collect()
    }

    pub(crate) fn status_of(&self, c: &DerivedClaim) -> Option<&ClaimStatus> {
        self.status.claims.iter().find(|s| s.resp_id == c.id)
    }

    /// A claim's status in words: `proven`, `stale`, `failing` or `planned`,
    /// and the probe mark when a probe or mutation run left one.
    pub(crate) fn claim_status(&self, c: &DerivedClaim) -> String {
        let s = self.status_of(c);
        let state = match (c.planned, s.map(|s| s.state)) {
            (true, _) | (_, Some(ClaimState::Planned)) => "planned",
            (_, Some(ClaimState::Proven)) => "proven",
            (_, Some(ClaimState::Failing)) => "failing",
            (_, Some(ClaimState::Stale)) | (_, None) => "stale",
        };
        match s.and_then(|s| s.probe) {
            Some(ProbeMark::Crosshair) => format!("{state}, a break survived"),
            Some(ProbeMark::DoubleCheck) => format!("{state}, probed"),
            None => state.to_string(),
        }
    }

    /// What a probe or mutation run found on a node's own code, in words.
    pub(crate) fn node_probe(&self, node_id: &str) -> Option<&'static str> {
        match self.status.nodes.get(node_id)?.probe? {
            ProbeMark::Crosshair => Some("a break survived in this code"),
            ProbeMark::DoubleCheck => Some("every break in this code was caught"),
        }
    }

    /// One line per claim: status, the claim, where its test is.
    pub(crate) fn claim_line(&self, c: &DerivedClaim) -> String {
        let concerns: String = c.concerns.iter().map(|t| format!(" [{t}]")).collect();
        format!("[{}] {}{concerns} — {}:{}", self.claim_status(c), c.description, c.file, c.line)
    }
}

/// Refresh the project's derived tree (cached on file mtimes, so an unchanged
/// project costs a walk, not a parse) and derive each claim's status from the
/// recorded verdicts, probes and mutation runs.
pub(crate) fn load(r: &ModelRef) -> Result<Loaded, String> {
    let (tree, status) = scryer_extract::test_status::derived_status(r)?;
    let threads = scryer_core::comments::threads(r.project_path());
    Ok(Loaded { tree, status, styles: scryer_core::load_styles(r.project_path()), threads })
}

/// How many of a file's style violations a file entry lists before pointing
/// at `check`.
const FILE_VIOLATIONS_CAP: usize = 3;

/// What the declared style asks of one file, for an agent about to edit it:
/// the rule its layer is held to (`domain (library) may import: domain`),
/// the violations the check charges to the file (its own detail, capped),
/// and for a tooling container that it is held to none.
pub(crate) struct StyleGuidance {
    pub rule: Option<String>,
    pub violations: Vec<String>,
    pub tooling: Option<&'static str>,
}

pub(crate) fn style_guidance(loaded: &Loaded, node: Option<&Node>, file: &str) -> StyleGuidance {
    let tree = &loaded.tree;
    let model = &tree.planned;
    let tooling = node
        .and_then(|n| scryer_core::style::container_of(model, &n.id))
        .is_some_and(|c| c.tooling == Some(true))
        .then_some("tooling: not held to style rules");
    let rule = node.filter(|_| tooling.is_none()).and_then(|n| {
        let layer = n.layer.as_deref()?;
        let name = scryer_core::style::governing_style(model, &n.id)?;
        let def = loaded.styles.get(name)?;
        let allowed = def.allowed(layer);
        let allowed = if allowed.is_empty() { "nothing".to_string() } else { allowed.join(", ") };
        Some(format!("{layer} ({name}) may import: {allowed}"))
    });
    let charged: Vec<&str> =
        tree.style_violations.iter().filter(|v| v.file == file).map(|v| v.detail.as_str()).collect();
    let mut violations: Vec<String> =
        charged.iter().take(FILE_VIOLATIONS_CAP).map(|d| format!("breaks the style: {d}")).collect();
    if charged.len() > FILE_VIOLATIONS_CAP {
        violations.push(format!("{} more — scryer-mcp check", charged.len() - FILE_VIOLATIONS_CAP));
    }
    StyleGuidance { rule, violations, tooling }
}

/// The derived tree alone, without the status reads.
pub(crate) fn load_tree(r: &ModelRef) -> Result<DerivedTree, String> {
    scryer_extract::derived_tree::derived_tree(r.project_path())
}

/// The claims proven now, by derived claim id.
pub(crate) fn proven_claims(status: &scryer_core::derived_tree::DerivedStatus) -> Vec<String> {
    status
        .claims
        .iter()
        .filter(|c| c.state == scryer_core::derived_tree::ClaimState::Proven)
        .map(|c| c.resp_id.clone())
        .collect()
}

/// The project's todo tests, `(file, junit_name)`.
pub(crate) fn planned_tests(tree: &DerivedTree) -> Vec<(String, String)> {
    tree.claims.iter().filter(|c| c.planned).map(|c| (c.file.clone(), c.junit_name.clone())).collect()
}

#[derive(serde::Serialize, serde::Deserialize)]
struct EdgeCache {
    fingerprint: u64,
    edges: Vec<(String, String)>,
}

/// The project's file-level import graph, `(importer, imported)`, without
/// guessed edges. Cached beside the derived tree under the same fingerprint,
/// so it is re-extracted only when a file changed.
pub(crate) fn import_edges(r: &ModelRef, tree: &DerivedTree) -> Result<Vec<(String, String)>, String> {
    let path = r.dir().join(".import_edges.json");
    let cached = std::fs::read_to_string(&path).ok().and_then(|s| serde_json::from_str::<EdgeCache>(&s).ok());
    if let Some(c) = cached.filter(|c| c.fingerprint == tree.fingerprint) {
        return Ok(c.edges);
    }
    let ctx = scryer_extract::extract_context(r.project_path())?;
    let edges: Vec<(String, String)> =
        ctx.file_edges.into_iter().filter(|e| !e.guessed).map(|e| (e.src, e.dst)).collect();
    let cache = EdgeCache { fingerprint: tree.fingerprint, edges };
    // A failed cache write only costs the next read an extraction.
    if let Ok(json) = serde_json::to_string(&cache) {
        let _ = std::fs::create_dir_all(r.dir()).and_then(|_| std::fs::write(&path, json));
    }
    Ok(cache.edges)
}

/// The claims a change to `files` reaches, at every level.
pub(crate) fn reached_claims(r: &ModelRef, tree: &DerivedTree, files: &[String]) -> Vec<String> {
    if files.is_empty() {
        return Vec::new();
    }
    let edges = import_edges(r, tree).unwrap_or_default();
    radius(tree, &edges, files, true).into_iter().map(|h| h.claim.id.clone()).collect()
}

/// The test level that carries a claim at this C4 level.
pub(crate) fn test_level(level: ClaimLevel) -> &'static str {
    match level {
        ClaimLevel::System => "e2e",
        ClaimLevel::Container => "integration",
        ClaimLevel::Component => "unit",
    }
}

fn matches(pattern: &str, file: &str) -> bool {
    pattern == file || glob::Pattern::new(pattern).is_ok_and(|p| p.matches(file))
}

/// The node a project file belongs to: the innermost component that holds
/// it, else the one whose boundary matches, else the container. A test file
/// belongs where its claims nest.
pub(crate) fn file_node<'m>(tree: &DerivedTree, model: &'m ScryModel, file: &str) -> Option<&'m Node> {
    let by_id: HashMap<&str, &Node> = model.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    if let Some(n) = tree.file_nodes.get(file).and_then(|id| by_id.get(id.as_str())) {
        return Some(n);
    }
    let owner = |kind: Kind| {
        model
            .boundaries
            .iter()
            .filter(|(_, srcs)| srcs.iter().any(|s| matches(&s.pattern, file)))
            .filter_map(|(id, _)| by_id.get(id.as_str()).copied())
            .find(|n| n.kind == kind)
    };
    owner(Kind::Component)
        .or_else(|| {
            let home = tree.claims.iter().find(|c| c.file == file)?;
            by_id.get(home.node_id.as_str()).copied()
        })
        .or_else(|| owner(Kind::Container))
}

/// The chain from the root down to `id`, by name.
pub(crate) fn chain(model: &ScryModel, id: &str) -> String {
    crate::helpers::breadcrumb(model, id)
}

/// The claims that speak for `file`: the tests written in it, then the
/// claims of the node it belongs to.
pub(crate) fn file_claims<'t>(tree: &'t DerivedTree, node: Option<&Node>, file: &str) -> Vec<&'t DerivedClaim> {
    let mut seen: HashSet<&str> = HashSet::new();
    tree.claims
        .iter()
        .filter(|c| c.file == file)
        .chain(tree.claims.iter().filter(|c| node.is_some_and(|n| n.id == c.node_id)))
        .filter(|c| seen.insert(c.id.as_str()))
        .collect()
}

/// Where a new todo stub for `file` belongs: a unit test file the component
/// already has (the one named after `file` first, then the nearest by path),
/// else the language's conventional place next to it.
pub(crate) fn stub_file(tree: &DerivedTree, node: Option<&Node>, file: &str) -> Option<String> {
    let stem = file_stem(file);
    let dir = parent(file);
    let existing: BTreeSet<&str> = tree
        .claims
        .iter()
        .filter(|c| c.level == ClaimLevel::Component && node.is_some_and(|n| n.id == c.node_id))
        .map(|c| c.file.as_str())
        .collect();
    if existing.contains(file) {
        return Some(file.to_string());
    }
    if let Some(named) = existing.iter().find(|f| tested_stem(f) == stem) {
        return Some(named.to_string());
    }
    if let Some(nearest) = existing.iter().max_by_key(|f| common_prefix(f, dir)) {
        return Some(nearest.to_string());
    }
    conventional_test_file(tree, file)
}

/// The conventional unit-test location for `file` by language, following the
/// project's own `.spec`/`.test` habit for JS/TS.
fn conventional_test_file(tree: &DerivedTree, file: &str) -> Option<String> {
    let (dir, stem) = (parent(file), file_stem(file));
    let at = |name: String| if dir.is_empty() { name } else { format!("{dir}/{name}") };
    let ext = file.rsplit_once('.').map(|(_, e)| e)?;
    Some(match ext {
        // Rust unit tests live in the file's own `#[cfg(test)] mod tests`.
        "rs" => file.to_string(),
        "ts" | "tsx" | "js" | "jsx" | "mts" | "mjs" => {
            let spec = tree.claims.iter().filter(|c| c.file.contains(".spec.")).count();
            let test = tree.claims.iter().filter(|c| c.file.contains(".test.")).count();
            at(format!("{stem}.{}.{ext}", if spec > test { "spec" } else { "test" }))
        }
        "py" => at(format!("test_{stem}.py")),
        "go" => at(format!("{stem}_test.go")),
        "cs" => at(format!("{stem}Tests.cs")),
        _ => return None,
    })
}

fn file_stem(file: &str) -> &str {
    let name = file.rsplit('/').next().unwrap_or(file);
    name.split('.').next().unwrap_or(name)
}

/// The product stem a test file is named after: `draft.spec.ts` → `draft`,
/// `test_draft.py` → `draft`, `DraftTests.cs` → `Draft`, `draft_test.go` →
/// `draft`.
fn tested_stem(file: &str) -> &str {
    let stem = file_stem(file);
    let stem = stem.strip_prefix("test_").unwrap_or(stem);
    let stem = stem.strip_suffix("_test").unwrap_or(stem);
    stem.strip_suffix("Tests").or_else(|| stem.strip_suffix("Test")).unwrap_or(stem)
}

fn parent(file: &str) -> &str {
    file.rsplit_once('/').map(|(d, _)| d).unwrap_or("")
}

fn common_prefix(a: &str, b: &str) -> usize {
    a.split('/').zip(b.split('/')).take_while(|(x, y)| x == y).count()
}

/// One test the radius reaches.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RadiusHit<'t> {
    pub claim: &'t DerivedClaim,
}

/// The tests a change to `files` reaches: those whose file imports a
/// changed file, directly or through others (or is one). A test whose file
/// resolves no import at all — its language or layout hides the link —
/// falls back by level: an integration test is reached by any change inside
/// its container, an e2e test by any change to product code. E2e tests only
/// when `with_e2e`; todo stubs never, there is nothing to run.
pub(crate) fn radius<'t>(
    tree: &'t DerivedTree,
    edges: &[(String, String)],
    files: &[String],
    with_e2e: bool,
) -> Vec<RadiusHit<'t>> {
    let reached = reach(edges, files);
    let model = &tree.planned;
    let holds = |node_id: &str| {
        model
            .boundaries
            .get(node_id)
            .is_some_and(|srcs| reached.iter().any(|f| srcs.iter().any(|s| matches(&s.pattern, f))))
    };
    let any_product = model
        .nodes
        .iter()
        .filter(|n| n.kind == Kind::Container)
        .any(|n| holds(&n.id));
    let imports = |file: &str| edges.iter().any(|(src, _)| src == file);
    tree.claims
        .iter()
        .filter(|c| !c.planned)
        .filter(|c| match c.level {
            ClaimLevel::Component => reached.contains(c.file.as_str()),
            ClaimLevel::Container => {
                reached.contains(c.file.as_str()) || (!imports(&c.file) && holds(&c.node_id))
            }
            ClaimLevel::System => {
                with_e2e && (reached.contains(c.file.as_str()) || (!imports(&c.file) && any_product))
            }
        })
        .map(|claim| RadiusHit { claim })
        .collect()
}

/// The files a change to `files` reaches: those files, and every file that
/// imports one of them, directly or through others.
pub(crate) fn reach<'e>(edges: &'e [(String, String)], files: &'e [String]) -> HashSet<&'e str> {
    let mut importers: HashMap<&str, Vec<&str>> = HashMap::new();
    for (src, dst) in edges {
        importers.entry(dst.as_str()).or_default().push(src.as_str());
    }
    let mut reached: HashSet<&str> = files.iter().map(String::as_str).collect();
    let mut frontier: Vec<&str> = reached.iter().copied().collect();
    while let Some(f) = frontier.pop() {
        for &imp in importers.get(f).into_iter().flatten() {
            if reached.insert(imp) {
                frontier.push(imp);
            }
        }
    }
    reached
}

/// The claim a test name points at: its JUnit name, or its identifier.
pub(crate) fn claim_for_test<'t>(tree: &'t DerivedTree, test: &str) -> Result<&'t DerivedClaim, String> {
    let test = test.trim();
    let by_junit: Vec<&DerivedClaim> = tree.claims.iter().filter(|c| c.junit_name == test).collect();
    let hits = if by_junit.is_empty() {
        tree.claims.iter().filter(|c| c.identifier == test).collect()
    } else {
        by_junit
    };
    match hits.as_slice() {
        [one] => Ok(one),
        [] => Err(format!("No test named '{test}' — pass its JUnit name as locate or orient lists it.")),
        many => Err(format!(
            "'{test}' names {} tests; pass the JUnit name of one: {}",
            many.len(),
            many.iter().map(|c| c.junit_name.as_str()).collect::<Vec<_>>().join(", ")
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn claim(file: &str, node: &str, level: ClaimLevel, planned: bool) -> DerivedClaim {
        DerivedClaim {
            id: format!("resp-{file}"),
            node_id: node.into(),
            level,
            file: file.into(),
            line: 1,
            identifier: "t".into(),
            description: "does it".into(),
            concerns: Vec::new(),
            junit_name: format!("{file}::t"),
            planned,
        }
    }

    fn tree(claims: Vec<DerivedClaim>) -> DerivedTree {
        DerivedTree {
            fingerprint: 0,
            committed: ScryModel::new(),
            planned: ScryModel::new(),
            claims,
            doc_files: Default::default(),
            file_nodes: Default::default(),
            dark_files: Vec::new(),
            style_violations: Vec::new(),
            layer_exempt: Vec::new(),
            tooling: Default::default(),
        }
    }

    /// When a radius is computed, reaches tests by import transitively, an integration test with no resolved import by any change in its container, e2e only on request, and stubs never.
    #[test]
    fn the_radius_follows_imports_and_levels() {
        let mut t = tree(vec![
            claim("src/a.test.ts", "c", ClaimLevel::Component, false),
            claim("tests/api.int.ts", "k", ClaimLevel::Container, false),
            claim("e2e/flow.ts", "s", ClaimLevel::System, false),
            claim("src/b.test.ts", "c", ClaimLevel::Component, false),
            claim("src/todo.test.ts", "c", ClaimLevel::Component, true),
        ]);
        t.planned
            .nodes
            .push(serde_json::from_value(serde_json::json!({ "id": "k", "kind": "container", "name": "api" })).unwrap());
        t.planned
            .boundaries
            .insert("k".into(), vec![scryer_core::Source { pattern: "src/**".into(), comment: None }]);
        let e = |a: &str, b: &str| (a.to_string(), b.to_string());
        let edges = vec![
            e("src/a.test.ts", "src/a.ts"),
            e("src/todo.test.ts", "src/a.ts"),
            e("src/b.test.ts", "src/b.ts"),
        ];
        let files = |hits: Vec<RadiusHit>| hits.iter().map(|h| h.claim.file.clone()).collect::<Vec<_>>();
        let changed = vec!["src/a.ts".to_string()];
        assert_eq!(files(radius(&t, &edges, &changed, false)), vec!["src/a.test.ts", "tests/api.int.ts"]);
        assert_eq!(
            files(radius(&t, &edges, &changed, true)),
            vec!["src/a.test.ts", "tests/api.int.ts", "e2e/flow.ts"]
        );
        assert!(radius(&t, &edges, &["docs/x.md".to_string()], true).is_empty(), "nothing in a container changed");
    }

    /// When a stub is placed, puts it in the test file named after the component's file, its nearest test file, or the language's convention, the first that exists.
    #[test]
    fn stubs_go_to_the_components_test_file_or_the_convention() {
        let mut model = ScryModel::new();
        model
            .nodes
            .push(serde_json::from_value(serde_json::json!({ "id": "c", "kind": "component", "name": "c" })).unwrap());
        let node = model.nodes.first();
        let t = tree(vec![
            claim("src/draft/save.spec.ts", "c", ClaimLevel::Component, false),
            claim("src/draft/merge.spec.ts", "c", ClaimLevel::Component, false),
        ]);
        assert_eq!(stub_file(&t, node, "src/draft/merge.ts").as_deref(), Some("src/draft/merge.spec.ts"));
        assert!(stub_file(&t, node, "src/draft/other.ts").unwrap().ends_with(".spec.ts"));
        let empty = tree(vec![claim("src/x.spec.ts", "elsewhere", ClaimLevel::Component, false)]);
        assert_eq!(stub_file(&empty, node, "src/a/b.ts").as_deref(), Some("src/a/b.spec.ts"));
        assert_eq!(stub_file(&empty, node, "crates/core/src/draft.rs").as_deref(), Some("crates/core/src/draft.rs"));
        assert_eq!(stub_file(&empty, node, "pkg/store.py").as_deref(), Some("pkg/test_store.py"));
        assert_eq!(stub_file(&empty, node, "README.md"), None);
    }

    /// When a test is looked up, finds it by its JUnit name, or by its identifier if that is unique.
    #[test]
    fn a_test_is_found_by_junit_name_or_identifier() {
        let t = tree(vec![
            claim("a.rs", "c", ClaimLevel::Component, false),
            claim("b.rs", "c", ClaimLevel::Component, false),
        ]);
        assert_eq!(claim_for_test(&t, "a.rs::t").unwrap().file, "a.rs");
        assert!(claim_for_test(&t, "t").unwrap_err().contains("names 2 tests"));
        assert!(claim_for_test(&t, "nope").is_err());
    }
}
