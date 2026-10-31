//! A piece of the architecture as a template: the model of a subtree, not
//! its code. [domain]
//!
//! A template holds what the model says and the code only carries: each
//! container's packaging, style and layers, each component's job, layer and
//! decisions, and each claim. Symbols, links and status are left out; they
//! are what an agent builds and the claims verify. What a template holds is
//! exactly what survives a round trip: scaffolded into an empty directory
//! and derived again, it gives back the same template. Anything that would
//! not survive that does not belong in it.
//!
//! Components are keyed by name and nesting, never by file path, so the
//! scaffolder can write them in the destination language's conventions.
//!
//! The contract is the one part that does not round-trip, by design: the
//! claims the source's other code made where it used the subtree (a
//! gateway's "If the token has expired, then rejects with 401"). They
//! belong to whatever uses the template in the destination, so the importing
//! agent places them there, and they verify that it wired the template in.
//! A claim joins the contract when its test exercises a symbol in the
//! subtree, or when it sits on a component that imports the subtree.
//!
//! A template keeps the source project's wording. Carrying it into another
//! project's nouns, in whatever language that project writes, is the
//! importing agent's job, not a word substitution.

use crate::derivation::Structure;
use scryer_core::derived_tree::{ClaimLevel, DerivedClaim, DerivedTree};
use scryer_core::{Kind, Node};
use serde::{Deserialize, Serialize};

/// The template format's version, bumped when a field changes meaning.
pub const TEMPLATE_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Template {
    pub scryer_template: u32,
    pub containers: Vec<TemplateContainer>,
    /// Claims for whatever uses the template, placed by the importing agent.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub contract: Vec<ContractClaim>,
}

/// A claim the source made outside the subtree about using it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContractClaim {
    #[serde(flatten)]
    pub claim: TemplateClaim,
    /// The template's components (or containers) the claim's code used.
    pub uses: Vec<String>,
    /// The source container the claim sat in, a hint to where it belongs.
    pub from: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateContainer {
    /// Its directory, which is also its key in `structure.toml`.
    pub key: String,
    /// The package name its manifest declares.
    pub package: String,
    /// The language it was exported from, the scaffolder's default.
    pub language: String,
    /// The display name `structure.toml` gives it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub layers: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decisions: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub claims: Vec<TemplateClaim>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub components: Vec<TemplateComponent>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateComponent {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decisions: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub claims: Vec<TemplateClaim>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub components: Vec<TemplateComponent>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateClaim {
    /// The claim, concern tags stripped.
    pub text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub concerns: Vec<String>,
    pub level: ClaimLevel,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decisions: Option<String>,
}

impl TemplateClaim {
    /// The claim as a test description writes it, tags appended.
    pub fn sentence(&self) -> String {
        let tags: String = self.concerns.iter().map(|t| format!(" [{t}]")).collect();
        format!("{}{tags}", self.text)
    }
}

/// The template of the subtree at `root`: a node id, or a container's key or
/// a node's name. The system gives every container; a container itself and
/// everything under it; a component itself and everything under it, inside
/// the shells of its container and the components above it, which keep what
/// it needs to land (packaging, style, layers, its parents' layers) and
/// nothing else.
pub fn template_of(tree: &DerivedTree, structure: &Structure, root: &str) -> Result<Template, String> {
    let model = &tree.planned;
    let root = find(tree, structure, root).ok_or_else(|| format!("no node '{root}' in the derived tree"))?;
    let children = |id: &str| -> Vec<&Node> { model.nodes.iter().filter(|n| n.parent_id.as_deref() == Some(id)).collect() };
    let containers: Vec<&Node> = match root.kind {
        Kind::System => children(&root.id).into_iter().filter(|n| n.kind == Kind::Container && n.external != Some(true)).collect(),
        Kind::Container | Kind::Component => vec![container_of(tree, root).ok_or("the node sits in no container")?],
        _ => return Err(format!("'{}' is a {:?}; a template starts at the system, a container or a component", root.name, root.kind)),
    };
    let mut out = Vec::new();
    for c in containers {
        let mut t = container(tree, structure, c)?;
        if root.kind == Kind::Component {
            t.description = None;
            t.decisions = None;
            t.claims.clear();
            t.components = vec![shell_path(tree, root)];
        }
        out.push(t);
    }
    out.sort_by(|a, b| a.key.cmp(&b.key));
    let contract = if root.kind == Kind::System { Vec::new() } else { contract(tree, &root.id) };
    Ok(Template { scryer_template: TEMPLATE_VERSION, containers: out, contract })
}

fn in_subtree(tree: &DerivedTree, root: &str, id: &str) -> bool {
    let mut cur = node(tree, id);
    while let Some(n) = cur {
        if n.id == root {
            return true;
        }
        cur = n.parent_id.as_deref().and_then(|p| node(tree, p));
    }
    false
}

/// The innermost component or container holding `id`, by name: a symbol's
/// owner, or the node itself.
fn owner_name(tree: &DerivedTree, id: &str) -> Option<String> {
    let mut cur = node(tree, id)?;
    while cur.kind == Kind::Symbol {
        cur = node(tree, cur.parent_id.as_deref()?)?;
    }
    Some(cur.name.clone())
}

/// The claims outside the subtree at `root` that exercise or import it.
fn contract(tree: &DerivedTree, root: &str) -> Vec<ContractClaim> {
    let model = &tree.planned;
    let inside = |id: &str| in_subtree(tree, root, id);
    // Claim id → what in the subtree its test names.
    let mut uses: std::collections::BTreeMap<&str, std::collections::BTreeSet<String>> = Default::default();
    for n in model.nodes.iter().filter(|n| inside(&n.id)) {
        for claim in &n.exercised_by {
            uses.entry(claim).or_default().extend(owner_name(tree, &n.id));
        }
    }
    // Node outside → what in the subtree its code imports. Only links from
    // code: a container's summary link, which has no kind, stands for all
    // of its code.
    let mut imports: std::collections::BTreeMap<&str, std::collections::BTreeSet<String>> = Default::default();
    for l in model.links.iter().filter(|l| l.kind.is_some() && !inside(&l.src) && inside(&l.dst)) {
        imports.entry(&l.src).or_default().extend(owner_name(tree, &l.dst));
    }
    let mut out: Vec<(&DerivedClaim, ContractClaim)> = Vec::new();
    for c in tree.claims.iter().filter(|c| !inside(&c.node_id)) {
        let mut used = uses.get(c.id.as_str()).cloned().unwrap_or_default();
        // A container's import is its root file's, so it speaks for that
        // file's unit claims and not for the container's integration tests.
        let on_container = node(tree, &c.node_id).is_some_and(|n| n.kind == Kind::Container);
        if !on_container || c.level == ClaimLevel::Component {
            used.extend(imports.get(c.node_id.as_str()).into_iter().flatten().cloned());
        }
        if used.is_empty() {
            continue;
        }
        let from = node(tree, &c.node_id)
            .and_then(|n| container_of(tree, n))
            .map_or_else(|| "the system".to_string(), |k| k.name.clone());
        let claim = TemplateClaim {
            text: c.description.clone(),
            concerns: c.concerns.clone(),
            level: c.level,
            decisions: c.decisions.clone(),
        };
        out.push((c, ContractClaim { claim, uses: used.into_iter().collect(), from }));
    }
    out.sort_by(|(a, x), (b, y)| (&x.from, &a.file, a.line).cmp(&(&y.from, &b.file, b.line)));
    out.into_iter().map(|(_, c)| c).collect()
}

fn find<'t>(tree: &'t DerivedTree, structure: &Structure, key: &str) -> Option<&'t Node> {
    let nodes = &tree.planned.nodes;
    nodes
        .iter()
        .find(|n| n.id == key)
        .or_else(|| nodes.iter().find(|n| n.kind == Kind::Container && container_dir(tree, &n.id) == key))
        .or_else(|| {
            // A `structure.toml` key naming a container by package or display name.
            let named = structure.containers.get(key).and_then(|c| c.name.as_deref()).unwrap_or(key);
            nodes.iter().find(|n| matches!(n.kind, Kind::Container | Kind::Component | Kind::System) && n.name == named)
        })
}

fn node<'t>(tree: &'t DerivedTree, id: &str) -> Option<&'t Node> {
    tree.planned.nodes.iter().find(|n| n.id == id)
}

fn container_of<'t>(tree: &'t DerivedTree, mut n: &'t Node) -> Option<&'t Node> {
    while n.kind != Kind::Container {
        n = node(tree, n.parent_id.as_deref()?)?;
    }
    Some(n)
}

/// A container's directory, from its boundary glob.
fn container_dir(tree: &DerivedTree, id: &str) -> String {
    tree.planned
        .boundaries
        .get(id)
        .and_then(|b| b.first())
        .map(|s| s.pattern.trim_end_matches("**").trim_end_matches('/').to_string())
        .unwrap_or_default()
}

fn language(file: &str) -> &'static str {
    match file.rsplit('.').next().unwrap_or("") {
        "rs" => "rust",
        "ts" | "tsx" | "mts" | "cts" => "typescript",
        "js" | "jsx" | "mjs" | "cjs" => "javascript",
        "py" => "python",
        "go" => "go",
        "cs" => "csharp",
        "java" => "java",
        "kt" | "kts" => "kotlin",
        "rb" => "ruby",
        "php" => "php",
        "clj" | "cljs" | "cljc" => "clojure",
        "c" | "h" => "c",
        "cc" | "cpp" | "hpp" => "cpp",
        _ => "unknown",
    }
}

fn container(tree: &DerivedTree, structure: &Structure, c: &Node) -> Result<TemplateContainer, String> {
    let key = container_dir(tree, &c.id);
    let config = structure.container(&c.name, &key);
    let display = config.and_then(|k| k.name.clone());
    let package = match &display {
        Some(_) => key.rsplit('/').next().filter(|s| !s.is_empty()).unwrap_or(&c.name).to_string(),
        None => c.name.clone(),
    };
    let entry = tree.doc_files.get(&c.id).map(String::as_str).unwrap_or("");
    Ok(TemplateContainer {
        key,
        package,
        language: language(entry).to_string(),
        name: display,
        style: c.style.clone(),
        layers: config.map(|k| k.layers.clone()).unwrap_or_default(),
        description: c.description.clone(),
        decisions: c.decisions.clone(),
        claims: claims(tree, &c.id),
        components: components(tree, &c.id),
    })
}

fn components(tree: &DerivedTree, parent: &str) -> Vec<TemplateComponent> {
    let mut out: Vec<TemplateComponent> = tree
        .planned
        .nodes
        .iter()
        .filter(|n| n.kind == Kind::Component && n.parent_id.as_deref() == Some(parent))
        .map(|n| TemplateComponent {
            name: n.name.clone(),
            layer: n.layer.clone(),
            description: n.description.clone(),
            decisions: n.decisions.clone(),
            claims: claims(tree, &n.id),
            components: components(tree, &n.id),
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// The component at `n` whole, inside the shells of the components above it.
fn shell_path(tree: &DerivedTree, n: &Node) -> TemplateComponent {
    let mut t = TemplateComponent {
        name: n.name.clone(),
        layer: n.layer.clone(),
        description: n.description.clone(),
        decisions: n.decisions.clone(),
        claims: claims(tree, &n.id),
        components: components(tree, &n.id),
    };
    let mut cur = n;
    while let Some(parent) = cur.parent_id.as_deref().and_then(|p| node(tree, p)).filter(|p| p.kind == Kind::Component) {
        t = TemplateComponent {
            name: parent.name.clone(),
            layer: parent.layer.clone(),
            description: None,
            decisions: None,
            claims: Vec::new(),
            components: vec![t],
        };
        cur = parent;
    }
    t
}

fn level_rank(l: ClaimLevel) -> u8 {
    match l {
        ClaimLevel::System => 0,
        ClaimLevel::Container => 1,
        ClaimLevel::Component => 2,
    }
}

/// The claims nesting under a node, by level and then in the order its test
/// files hold them.
fn claims(tree: &DerivedTree, node_id: &str) -> Vec<TemplateClaim> {
    let mut own: Vec<&DerivedClaim> = tree.claims.iter().filter(|c| c.node_id == node_id).collect();
    own.sort_by(|a, b| (level_rank(a.level), &a.file, a.line).cmp(&(level_rank(b.level), &b.file, b.line)));
    own.into_iter()
        .map(|c| TemplateClaim {
            text: c.description.clone(),
            concerns: c.concerns.clone(),
            level: c.level,
            decisions: c.decisions.clone(),
        })
        .collect()
}

impl Template {
    /// The template with its one container moved to directory `key`; a
    /// package named after the old directory is named after the new one.
    pub fn moved(&self, key: &str) -> Result<Template, String> {
        let [c] = self.containers.as_slice() else {
            return Err(format!("a template of {} containers can't be moved to one directory; export them one at a time", self.containers.len()));
        };
        let key = key.trim_matches('/');
        let last = |k: &str| k.rsplit('/').next().unwrap_or(k).to_string();
        let mut c = c.clone();
        if c.package == last(&c.key) && !key.is_empty() {
            c.package = last(key);
        }
        c.key = key.to_string();
        Ok(Template { containers: vec![c], ..self.clone() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::derived_tree::{derived_tree_uncached, read_structure};
    use std::path::Path;

    fn write(dir: &Path, rel: &str, text: &str) {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    /// A hexagonal ledger crate with a domain and an infrastructure
    /// component, and a second crate beside it.
    fn project() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        write(d, "Cargo.toml", "[workspace]\nmembers = [\"crates/*\"]\n");
        write(d, "crates/ledger/Cargo.toml", "[package]\nname = \"ledger\"\nversion = \"0.1.0\"\n");
        write(
            d,
            "crates/ledger/src/lib.rs",
            "//! Keeps accounts' balances.\n//!\n//! Amounts are minor units.\npub mod account;\npub mod store;\n",
        );
        write(
            d,
            "crates/ledger/src/account.rs",
            "//! An account's balance, from its entries. [domain]\n\
             pub fn tally(e: &[i64]) -> i64 { e.iter().sum() }\n\
             #[cfg(test)]\nmod tests {\n    /// Sums the entries\n    #[test]\n    fn sums() { assert_eq!(super::tally(&[1, 2]), 3); }\n\n\
             \x20   /// If an entry overflows, then refuses the posting [safety]\n    ///\n    /// A wrapped balance is worse than none.\n    #[test]\n    #[ignore]\n    fn overflow() { todo!() }\n}\n",
        );
        write(d, "crates/ledger/src/store.rs", "//! Persists entries. [infrastructure]\npub fn put() {}\n");
        write(d, "crates/ledger/tests/claims.rs", "/// When an entry is posted, persists it\n#[test]\nfn posts() {}\n");
        write(
            d,
            "crates/web/Cargo.toml",
            "[package]\nname = \"web\"\nversion = \"0.1.0\"\n[dependencies]\nledger = { path = \"../ledger\" }\n",
        );
        write(
            d,
            "crates/web/src/lib.rs",
            "//! Serves pages.\nuse ledger::store::put;\npub mod nav;\npub mod page;\npub fn home() -> u16 { put(); 200 }\n",
        );
        write(
            d,
            "crates/web/src/page.rs",
            "//! Renders a balance.\nuse ledger::account::tally;\npub fn show() -> i64 { tally(&[1]) }\n\
             #[cfg(test)]\nmod tests {\n    /// Shows the account's balance [ui]\n    #[test]\n    fn shows() { assert_eq!(super::show(), 1); }\n}\n",
        );
        write(
            d,
            "crates/web/src/nav.rs",
            "//! Lists the pages.\npub fn links() -> usize { 2 }\n\
             #[cfg(test)]\nmod tests {\n    /// Lists every page\n    #[test]\n    fn lists() { assert_eq!(super::links(), 2); }\n}\n",
        );
        write(
            d,
            "crates/web/tests/claims.rs",
            "use ledger::store::put;\n\n/// When a page is saved, stores it\n#[test]\nfn saves() { put(); }\n\n/// Serves the home page\n#[test]\nfn serves() { assert_eq!(web::home(), 200); }\n",
        );
        write(
            d,
            ".scryer/structure.toml",
            "[containers.\"crates/ledger\"]\nname = \"Ledger\"\nstyle = \"hexagonal\"\nlayers = [\"domain\", \"infrastructure\"]\n",
        );
        dir
    }

    fn template(dir: &Path, root: &str) -> Template {
        let tree = derived_tree_uncached(dir).unwrap();
        let s = read_structure(dir).unwrap();
        template_of(&tree, &s, root).unwrap()
    }

    fn claim(text: &str, level: ClaimLevel) -> TemplateClaim {
        TemplateClaim { text: text.into(), concerns: Vec::new(), level, decisions: None }
    }

    fn component(name: &str, layer: &str, description: &str, claims: &[&str]) -> TemplateComponent {
        TemplateComponent {
            name: name.into(),
            layer: Some(layer.into()),
            description: Some(description.into()),
            decisions: None,
            claims: claims.iter().map(|c| claim(c, ClaimLevel::Component)).collect(),
            components: Vec::new(),
        }
    }

    /// A ledger template written out by hand.
    fn sample() -> Template {
        let posted = TemplateClaim {
            concerns: vec!["persistence".into()],
            decisions: Some("A ledger never forgets.".into()),
            ..claim("When an entry is posted, persists it", ClaimLevel::Container)
        };
        Template {
            contract: Vec::new(),
            scryer_template: TEMPLATE_VERSION,
            containers: vec![TemplateContainer {
                key: "crates/ledger".into(),
                package: "ledger".into(),
                language: "rust".into(),
                name: Some("Ledger".into()),
                style: Some("hexagonal".into()),
                layers: vec!["domain".into(), "infrastructure".into()],
                description: Some("Keeps a ledger of entries.".into()),
                decisions: Some("Every LEDGER entry is immutable.".into()),
                claims: vec![posted],
                components: vec![
                    component(
                        "account",
                        "domain",
                        "An account's balance, from its entries.",
                        &["Sums the entries", "If an entry overflows, then refuses the posting"],
                    ),
                    TemplateComponent {
                        decisions: Some("Ledger rows are append-only.".into()),
                        ..component("entry_store", "infrastructure", "Stores ledger entries.", &["Puts an entry"])
                    },
                ],
            }],
        }
    }

    /// When a container is exported, keeps its style, layers, description, decisions, components and claims
    #[test]
    fn keeps_a_containers_model() {
        let dir = project();
        let t = template(dir.path(), "crates/ledger");
        assert_eq!(t.containers.len(), 1);
        let c = &t.containers[0];
        assert_eq!((c.key.as_str(), c.package.as_str(), c.language.as_str()), ("crates/ledger", "ledger", "rust"));
        assert_eq!(c.name.as_deref(), Some("Ledger"));
        assert_eq!(c.style.as_deref(), Some("hexagonal"));
        assert_eq!(c.layers, ["domain", "infrastructure"]);
        assert_eq!(c.description.as_deref(), Some("Keeps accounts' balances."));
        assert_eq!(c.decisions.as_deref(), Some("Amounts are minor units."));
        assert_eq!(c.claims.len(), 1);
        assert_eq!((c.claims[0].text.as_str(), c.claims[0].level), ("When an entry is posted, persists it", ClaimLevel::Container));
        let names: Vec<_> = c.components.iter().map(|x| (x.name.as_str(), x.layer.as_deref())).collect();
        assert_eq!(names, [("account", Some("domain")), ("store", Some("infrastructure"))]);
        let account = &c.components[0];
        assert_eq!(account.description.as_deref(), Some("An account's balance, from its entries."));
        let overflow = &account.claims[1];
        assert_eq!(overflow.text, "If an entry overflows, then refuses the posting");
        assert_eq!(overflow.concerns, ["safety"]);
        assert_eq!(overflow.decisions.as_deref(), Some("A wrapped balance is worse than none."));
        assert_eq!(overflow.sentence(), "If an entry overflows, then refuses the posting [safety]");
    }

    /// Leaves out what the code implements: symbols, links, file positions and status
    #[test]
    fn leaves_out_the_implementation() {
        let dir = project();
        let json = serde_json::to_string(&template(dir.path(), "crates/ledger")).unwrap();
        for absent in ["tally", "account.rs", "\"line\"", "planned", "proven", "junit"] {
            assert!(!json.contains(absent), "{absent} leaked into {json}");
        }
    }

    /// When a component is exported, keeps its container's style and layers but none of its other components
    #[test]
    fn keeps_a_component_in_its_container_shell() {
        let dir = project();
        let tree = derived_tree_uncached(dir.path()).unwrap();
        let account = tree.planned.nodes.iter().find(|n| n.name == "account").unwrap();
        let s = read_structure(dir.path()).unwrap();
        let t = template_of(&tree, &s, &account.id).unwrap();
        let c = &t.containers[0];
        assert_eq!(c.style.as_deref(), Some("hexagonal"));
        assert_eq!(c.layers, ["domain", "infrastructure"]);
        assert!(c.description.is_none() && c.decisions.is_none() && c.claims.is_empty());
        let names: Vec<_> = c.components.iter().map(|x| x.name.as_str()).collect();
        assert_eq!(names, ["account"]);
        assert_eq!(c.components[0].claims.len(), 2);
    }

    /// When the system is exported, keeps every container
    #[test]
    fn keeps_every_container_of_the_system() {
        let dir = project();
        let tree = derived_tree_uncached(dir.path()).unwrap();
        let system = tree.planned.nodes.iter().find(|n| n.kind == Kind::System).unwrap();
        let s = read_structure(dir.path()).unwrap();
        let t = template_of(&tree, &s, &system.id).unwrap();
        let keys: Vec<_> = t.containers.iter().map(|c| c.key.as_str()).collect();
        assert_eq!(keys, ["crates/ledger", "crates/web"]);
    }

    /// When a template's container is moved, takes the new directory and renames its package to match
    #[test]
    fn moves_a_container() {
        let t = sample().moved("services/billing").unwrap();
        assert_eq!((t.containers[0].key.as_str(), t.containers[0].package.as_str()), ("services/billing", "billing"));
        let mut own = sample();
        own.containers[0].package = "ledger-core".into();
        assert_eq!(own.moved("services/billing").unwrap().containers[0].package, "ledger-core");
    }

    /// If a template of several containers is moved, then refuses
    #[test]
    fn refuses_moving_several() {
        let mut t = sample();
        let mut web = t.containers[0].clone();
        web.key = "crates/web".into();
        t.containers.push(web);
        assert!(t.moved("services/billing").unwrap_err().contains("2 containers"));
    }

    /// When a subtree is exported, keeps as its contract the claims outside it whose tests exercise it or whose components import it, with what they use
    #[test]
    fn keeps_its_consumers_claims_as_its_contract() {
        let dir = project();
        let t = template(dir.path(), "crates/ledger");
        let got: Vec<_> = t.contract.iter().map(|c| (c.claim.sentence(), c.uses.clone(), c.from.as_str(), c.claim.level)).collect();
        assert_eq!(
            got,
            [
                ("Shows the account's balance [ui]".to_string(), vec!["account".to_string()], "web", ClaimLevel::Component),
                ("When a page is saved, stores it".to_string(), vec!["store".to_string()], "web", ClaimLevel::Container),
            ]
        );
        let tree = derived_tree_uncached(dir.path()).unwrap();
        let account = tree.planned.nodes.iter().find(|n| n.name == "account").unwrap();
        let t = template_of(&tree, &read_structure(dir.path()).unwrap(), &account.id).unwrap();
        let texts: Vec<_> = t.contract.iter().map(|c| c.claim.text.as_str()).collect();
        assert_eq!(texts, ["Shows the account's balance"], "a component's contract is what uses that component");
    }

    /// Leaves out of the contract a claim outside the subtree that neither exercises nor imports it
    #[test]
    fn leaves_unrelated_consumer_claims_out() {
        let dir = project();
        let t = template(dir.path(), "crates/ledger");
        assert!(t.contract.iter().all(|c| c.claim.text != "Lists every page"));
        assert!(
            t.contract.iter().all(|c| c.claim.text != "Serves the home page"),
            "a crate root's import speaks for its own unit claims, not the crate's integration tests"
        );
        let tree = derived_tree_uncached(dir.path()).unwrap();
        let system = tree.planned.nodes.iter().find(|n| n.kind == Kind::System).unwrap();
        let whole = template_of(&tree, &read_structure(dir.path()).unwrap(), &system.id).unwrap();
        assert!(whole.contract.is_empty(), "nothing is outside the system");
    }
}
