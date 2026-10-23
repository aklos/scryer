//! The derived C4 tree's entry points: the project's tree, cached on file
//! mtimes, or derived without touching the cache.

use crate::domain::structure::Structure;
use crate::infrastructure::derived_tree as store;
use scryer_core::derived_tree::DerivedTree;
use std::path::Path;

/// The project's derived tree: the cached one when no file changed since,
/// otherwise derived afresh and cached.
pub fn derived_tree(project: &Path) -> Result<DerivedTree, String> {
    store::derived_tree(project)
}

/// Derive the tree without touching the cache, for a project that must not
/// be written to.
pub fn derived_tree_uncached(project: &Path) -> Result<DerivedTree, String> {
    store::derived_tree_uncached(project)
}

/// The project's `structure.toml`, or defaults when there is none.
pub fn read_structure(project: &Path) -> Result<Structure, String> {
    store::read_structure(project)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::claims;
    use scryer_core::derived_tree::{read_derived_tree_at, ClaimLevel};
    use scryer_core::{scan, ModelRef};
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::{Path, PathBuf};
    use scryer_core::Kind;

    fn write(dir: &Path, rel: &str, text: &str) {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn cargo_workspace() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        write(d, "Cargo.toml", "[workspace]\nmembers = [\"crates/*\"]\n");
        write(d, "crates/core/Cargo.toml", "[package]\nname = \"core\"\nversion = \"0.1.0\"\n");
        write(d, "crates/core/src/lib.rs", "//! The domain core.\npub mod draft;\npub mod store;\n");
        write(
            d,
            "crates/core/src/draft.rs",
            r#"//! Holds the user's draft.
use crate::store::Store;
pub struct Draft { pub text: String }
pub fn save(s: &Store, d: &Draft) -> bool { s.put(&d.text) }

#[cfg(test)]
mod tests {
    /// When a save fails, keeps the draft [persistence]
    #[test]
    fn keeps_draft() {}

    /// Merges two drafts
    #[test]
    fn merges() { todo!() }

    fn fixture() {}
}
"#,
        );
        write(d, "crates/core/src/store/mod.rs", "//! Persists text.\npub struct Store;\nimpl Store { pub fn put(&self, _t: &str) -> bool { true } }\n");
        write(
            d,
            "crates/core/tests/roundtrip.rs",
            "/// Saves and reloads a draft unchanged\n#[test]\nfn roundtrip() {}\n",
        );
        write(
            d,
            ".scryer/structure.toml",
            "[system]\nname = \"drafts\"\npersons = [\"writer\"]\n\n[levels]\ne2e = [\"e2e/**\"]\n\n[containers.core]\nname = \"Core\"\n",
        );
        write(d, "e2e/tests/flow.rs", "/// Writes and publishes a post\n#[test]\nfn flow() {}\n");
        dir
    }

    /// Claims nest by test level: unit under the component whose file holds
    /// the test, integration under the crate, e2e under the system; todo
    /// stubs are only in the planned layer.
    #[test]
    fn places_claims_by_level() {
        let dir = cargo_workspace();
        let tree = derived_tree(dir.path()).unwrap();
        let m = &tree.planned;
        let by_name = |name: &str| m.nodes.iter().find(|n| n.name == name).unwrap_or_else(|| panic!("no node {name}: {:?}", m.nodes.iter().map(|n| &n.name).collect::<Vec<_>>()));
        let system = by_name("drafts");
        assert_eq!(system.kind, Kind::System);
        assert_eq!(by_name("writer").kind, Kind::Person);
        let core = by_name("Core");
        assert_eq!(core.kind, Kind::Container);
        assert_eq!(core.description.as_deref(), Some("The domain core."));
        let draft = by_name("draft");
        assert_eq!(draft.kind, Kind::Component);
        assert_eq!(draft.parent_id.as_deref(), Some(core.id.as_str()));
        assert_eq!(draft.description.as_deref(), Some("Holds the user's draft."));
        assert_eq!(by_name("store").description.as_deref(), Some("Persists text."));

        let level_of = |desc: &str| {
            let c = tree.claims.iter().find(|c| c.description == desc).unwrap();
            (c.level, m.nodes.iter().find(|n| n.id == c.node_id).unwrap().name.clone(), c.planned)
        };
        assert_eq!(level_of("When a save fails, keeps the draft"), (ClaimLevel::Component, "draft".into(), false));
        assert_eq!(level_of("Merges two drafts"), (ClaimLevel::Component, "draft".into(), true));
        assert_eq!(level_of("Saves and reloads a draft unchanged"), (ClaimLevel::Container, "Core".into(), false));
        assert_eq!(level_of("Writes and publishes a post"), (ClaimLevel::System, "drafts".into(), false));

        let draft_claims: Vec<&str> = draft.responsibilities.iter().map(|r| r.statement.as_str()).collect();
        assert_eq!(draft_claims.len(), 2);
        let committed_draft = tree.committed.nodes.iter().find(|n| n.id == draft.id).unwrap();
        assert_eq!(committed_draft.responsibilities.len(), 1, "the todo stub is planned only");
        assert_eq!(draft.responsibilities[0].concern.as_deref(), Some("persistence"));

        // Symbols are the product definitions; test helpers stay out.
        let symbols: BTreeSet<&str> = m
            .nodes
            .iter()
            .filter(|n| n.kind == Kind::Symbol && n.parent_id.as_deref() == Some(draft.id.as_str()))
            .map(|n| n.name.as_str())
            .collect();
        assert_eq!(symbols, BTreeSet::from(["Draft", "save"]));
        // The import draft -> store becomes a component link.
        let store = by_name("store");
        assert!(m.links.iter().any(|l| l.src == draft.id && l.dst == store.id), "links: {:?}", m.links);

        // A unit claim anchors the file it exercises; the crate root only
        // declares modules a test reaches, so nothing is dark.
        let keeps = tree.claims.iter().find(|c| c.identifier == "keeps_draft").unwrap();
        assert_eq!(m.source_map[&keeps.id][0].pattern, "crates/core/src/draft.rs");
        assert_eq!(tree.file_nodes["crates/core/src/store/mod.rs"], store.id);
        assert!(tree.dark_files.is_empty(), "{:?}", tree.dark_files);
    }

    /// When a file inside a directory component has a module doc and claims
    /// of its own, it becomes a sub-component carrying both; a file with
    /// claims but no doc leaves them on the directory; a test whose code under
    /// test cannot be found gets a component named after its file.
    #[test]
    fn documented_tested_files_become_sub_components() {
        let dir = cargo_workspace();
        let d = dir.path();
        write(
            d,
            "crates/core/src/store/disk.rs",
            "//! Writes text to disk.\npub fn flush() {}\n#[cfg(test)]\nmod tests {\n    /// Flushes on close\n    #[test]\n    fn flushes() {}\n}\n",
        );
        write(
            d,
            "crates/core/src/store/cache.rs",
            "pub fn warm() {}\n#[cfg(test)]\nmod tests {\n    /// Warms lazily\n    #[test]\n    fn warms() {}\n}\n",
        );
        write(d, "package.json", r#"{"name":"web"}"#);
        write(d, "web/main.ts", "export function boot() { return 1; }\n");
        write(d, "tests/smoke.test.ts", "it('boots without a backend', () => {});\n");
        let tree = derived_tree_uncached(d).unwrap();
        let m = &tree.planned;
        let named = |name: &str| m.nodes.iter().find(|n| n.name == name).unwrap();
        let home_of = |desc: &str| {
            let c = tree.claims.iter().find(|c| c.description == desc).unwrap();
            m.nodes.iter().find(|n| n.id == c.node_id).unwrap()
        };

        let store = named("store");
        let disk = named("disk");
        assert_eq!(disk.kind, Kind::Component);
        assert_eq!(disk.parent_id.as_deref(), Some(store.id.as_str()));
        assert_eq!(disk.description.as_deref(), Some("Writes text to disk."));
        assert_eq!(home_of("Flushes on close").id, disk.id);
        assert_eq!(named("flush").parent_id.as_deref(), Some(disk.id.as_str()));
        assert!(m.nodes.iter().all(|n| n.name != "cache"), "an undocumented file is no sub-component");
        assert_eq!(home_of("Warms lazily").id, store.id);
        assert_eq!(named("warm").parent_id.as_deref(), Some(store.id.as_str()));

        let smoke = home_of("Boots without a backend");
        assert_eq!((smoke.kind, smoke.name.as_str()), (Kind::Component, "smoke"));
        let parent = m.nodes.iter().find(|n| Some(n.id.as_str()) == smoke.parent_id.as_deref()).unwrap();
        assert_eq!((parent.kind, parent.name.as_str()), (Kind::Container, "web"));
    }

    /// When a styled crate has files outside its source root, asks them for no layer, and still flags a source module that matches no declared layer.
    #[test]
    fn only_source_modules_are_asked_for_a_layer() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        write(d, "Cargo.toml", "[package]\nname = \"core\"\nversion = \"0.1.0\"\n");
        write(d, "build.rs", "fn main() {}\n");
        write(d, "examples/demo.rs", "fn main() {}\n");
        write(d, "src/lib.rs", "pub mod domain;\npub mod odd;\n");
        write(d, "src/domain/mod.rs", "pub fn rule() {}\n");
        write(d, "src/odd/mod.rs", "pub fn thing() {}\n");
        write(d, ".scryer/structure.toml", "[containers.\"\"]\nstyle = \"library\"\n");
        let tree = derived_tree_uncached(d).unwrap();
        let name_of = |id: &str| tree.planned.nodes.iter().find(|n| n.id == id).unwrap().name.clone();
        let layerless: BTreeSet<String> = tree
            .style_violations
            .iter()
            .filter(|v| v.kind == "layerless")
            .map(|v| name_of(&v.node_id))
            .collect();
        assert_eq!(layerless, BTreeSet::from(["odd".to_string()]), "{:?}", tree.style_violations);
        let exempt: BTreeSet<String> = tree.layer_exempt.iter().map(|id| name_of(id)).collect();
        // The build script is tooling, not a component at all.
        assert_eq!(exempt, BTreeSet::from(["examples".to_string()]));
    }

    /// When a container declares a project style from `.scryer/styles/`, its
    /// components take that style's layers by name, and a later change to
    /// the style file derives the tree again.
    #[test]
    fn a_project_style_gives_components_its_layers() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        write(d, "Cargo.toml", "[package]\nname = \"tool\"\nversion = \"0.1.0\"\n");
        write(d, "src/lib.rs", "pub mod front;\npub mod back;\n");
        write(d, "src/front/mod.rs", "pub fn a() { crate::back::b() }\n");
        write(d, "src/back/mod.rs", "pub fn b() {}\n");
        write(d, ".scryer/structure.toml", "[containers.\"\"]\nstyle = \"two-tier\"\n");
        let mut style = scryer_core::style::core_shell();
        style.name = "two-tier".into();
        style.layers[0].name = "front".into();
        style.layers[1].name = "back".into();
        style.matrix = [("front".to_string(), vec!["front".to_string(), "back".to_string()]), ("back".to_string(), vec!["back".to_string()])]
            .into_iter()
            .collect();
        write(d, ".scryer/styles/two-tier.json", &serde_json::to_string(&style).unwrap());
        let tree = derived_tree(d).unwrap();
        let layer = |name: &str| tree.planned.nodes.iter().find(|n| n.name == name).unwrap().layer.clone();
        assert_eq!(layer("front").as_deref(), Some("front"));
        assert_eq!(layer("back").as_deref(), Some("back"));
        assert!(tree.style_violations.iter().all(|v| v.kind != "layerless"), "{:?}", tree.style_violations);

        // A different length, so the file's size changes whatever the clock.
        style.layers[1].name = "backend".into();
        write(d, ".scryer/styles/two-tier.json", &serde_json::to_string(&style).unwrap());
        let again = derived_tree(d).unwrap();
        assert_eq!(again.planned.nodes.iter().find(|n| n.name == "back").unwrap().layer, None);
    }

    /// When a crate's root file imports another crate, the two containers
    /// are linked, and the link is held to the other container's style like
    /// a component's would be.
    #[test]
    fn a_crate_roots_imports_link_its_container() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        write(d, "Cargo.toml", "[workspace]\nmembers = [\"crates/*\"]\n");
        write(d, "crates/gateway/Cargo.toml", "[package]\nname = \"gateway\"\nversion = \"0.1.0\"\n[dependencies]\nledger = { path = \"../ledger\" }\n");
        write(d, "crates/gateway/src/lib.rs", "use ledger::infrastructure::db::open;\npub fn handle() { open() }\n");
        write(d, "crates/ledger/Cargo.toml", "[package]\nname = \"ledger\"\nversion = \"0.1.0\"\n");
        write(d, "crates/ledger/src/lib.rs", "pub mod domain;\npub mod infrastructure;\n");
        write(d, "crates/ledger/src/domain/mod.rs", "pub fn post() {}\n");
        write(d, "crates/ledger/src/infrastructure/mod.rs", "pub mod db;\n");
        write(d, "crates/ledger/src/infrastructure/db.rs", "pub fn open() {}\n");
        write(d, ".scryer/structure.toml", "[containers.ledger]\nstyle = \"library\"\n");
        let tree = derived_tree_uncached(d).unwrap();
        let m = &tree.planned;
        let id = |name: &str| m.nodes.iter().find(|n| n.name == name).unwrap().id.clone();
        let (gateway, ledger) = (id("gateway"), id("ledger"));
        assert!(m.links.iter().any(|l| l.src == gateway && l.dst == ledger), "{:?}", m.links);
        assert!(
            tree.style_violations.iter().any(|v| v.detail.contains("enters container") && v.detail.contains("infrastructure")),
            "{:?}",
            tree.style_violations
        );
    }

    /// When structure.toml relates persons, the system and externals, and a
    /// container says which external it uses, each becomes a link with its
    /// label, the container's use lifted to the system too; none needs an
    /// import behind it, and none is a style finding.
    #[test]
    fn declared_relationships_become_context_links() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        write(d, "Cargo.toml", "[workspace]\nmembers = [\"crates/*\"]\n");
        write(d, "crates/ledger/Cargo.toml", "[package]\nname = \"ledger\"\nversion = \"0.1.0\"\n");
        write(d, "crates/ledger/src/lib.rs", "pub mod domain;\n");
        write(d, "crates/ledger/src/domain/mod.rs", "pub fn post() {}\n");
        write(
            d,
            ".scryer/structure.toml",
            "[system]\nname = \"Pay\"\npersons = [\"merchant\"]\nexternals = [\"bank\", \"db\"]\n\n\
             [relationships]\nmerchant = [{ to = \"Pay\", label = \"integrates\" }]\n\"Pay\" = [\"bank\"]\n\n\
             [containers.ledger]\nstyle = \"library\"\nuses = [{ to = \"db\", label = \"stores entries in\" }]\n",
        );
        let tree = derived_tree_uncached(d).unwrap();
        let m = &tree.committed;
        let id = |name: &str| m.nodes.iter().find(|n| n.name == name).unwrap().id.clone();
        let link = |a: &str, b: &str| m.links.iter().find(|l| l.src == id(a) && l.dst == id(b)).map(|l| l.label.as_str());
        assert_eq!(link("merchant", "Pay"), Some("integrates"));
        assert_eq!(link("Pay", "bank"), Some(""));
        assert_eq!(link("ledger", "db"), Some("stores entries in"));
        assert_eq!(link("Pay", "db"), Some("stores entries in"));
        assert_eq!(link("Pay", "merchant"), None);
        assert!(tree.style_violations.is_empty(), "{:?}", tree.style_violations);
    }

    /// When a directory name starts with a dot, it never becomes a component or a container, and its files are neither symbols nor dark.
    #[test]
    fn dot_directories_are_never_the_product() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        write(d, "package.json", "{\"name\": \"web\"}\n");
        write(d, "src/app.ts", "export function boot() { return 1; }\n");
        write(d, ".claude/skills/run/scripts/wd.py", "def drive():\n    pass\n");
        write(d, ".github/actions/tag/package.json", "{\"name\": \"tag\"}\n");
        write(d, ".github/actions/tag/index.js", "export function tag() {}\n");
        let tree = derived_tree_uncached(d).unwrap();
        let m = &tree.planned;
        let names: Vec<&str> =
            m.nodes.iter().filter(|n| matches!(n.kind, Kind::Container | Kind::Component)).map(|n| n.name.as_str()).collect();
        assert!(names.iter().all(|n| !n.starts_with('.') && *n != "tag" && *n != "scripts"), "{names:?}");
        assert!(m.nodes.iter().all(|n| n.name != "drive" && n.name != "tag"), "no symbols from dot directories");
        assert!(tree.file_nodes.keys().chain(&tree.dark_files).all(|f| !f.starts_with('.')), "{:?}", tree.file_nodes);
    }

    /// When a tool's config file sits outside its package's source root, it is no component and not dark, while one inside `src/` is product code.
    #[test]
    fn tool_configs_outside_the_source_root_are_not_components() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        write(d, "package.json", "{\"name\": \"web\"}\n");
        write(d, "vite.config.ts", "export default { plugins: [] };\n");
        write(d, "harness/vite.config.ts", "export default {};\n");
        write(d, "src/content.config.ts", "export const collections = { docs: 1 };\n");
        let tree = derived_tree_uncached(d).unwrap();
        let components: BTreeSet<&str> =
            tree.planned.nodes.iter().filter(|n| n.kind == Kind::Component).map(|n| n.name.as_str()).collect();
        assert_eq!(components, BTreeSet::from(["content"]));
        assert!(tree.dark_files.iter().all(|f| !f.ends_with("vite.config.ts")), "{:?}", tree.dark_files);
    }

    /// When a symbol has a doc comment it is the node's description, and a data shape's properties carry each field's type and doc.
    #[test]
    fn symbols_carry_their_docs_and_typed_properties() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        write(d, "Cargo.toml", "[package]\nname = \"core\"\nversion = \"0.1.0\"\n");
        write(d, "src/lib.rs", "pub mod draft;\n");
        write(
            d,
            "src/draft.rs",
            "/// What the user types.\npub struct Draft {\n    /// The text, as typed.\n    pub text: String,\n}\n\n/// Saves a draft.\npub fn save(_d: &Draft) {}\n",
        );
        let tree = derived_tree_uncached(d).unwrap();
        let named = |n: &str| tree.planned.nodes.iter().find(|x| x.name == n).unwrap();
        let draft = named("Draft");
        assert_eq!(draft.description.as_deref(), Some("What the user types."));
        let p = &draft.properties[0];
        assert_eq!((p.label.as_str(), p.ty.as_deref(), p.description.as_str()), ("text", Some("String"), "The text, as typed."));
        assert_eq!(named("save").description.as_deref(), Some("Saves a draft."));
        assert!(named("save").properties.is_empty());
    }

    /// When a React function component takes props, they are its node's properties and it carries the React component technology, so it does not read as a data shape.
    #[test]
    fn a_react_components_props_are_its_properties() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        write(d, "package.json", "{\"name\": \"web\"}\n");
        write(
            d,
            "src/ui/Card.tsx",
            "interface CardProps {\n  /** Shown on top. */\n  title: string;\n  size?: number;\n}\n/** A card. */\nexport function Card({ title }: CardProps) { return <div>{title}</div>; }\n",
        );
        let tree = derived_tree_uncached(d).unwrap();
        let card = tree.planned.nodes.iter().find(|x| x.name == "Card").unwrap();
        assert_eq!(card.technology.as_deref(), Some(crate::domain::derive::REACT_COMPONENT));
        assert_eq!(card.description.as_deref(), Some("A card."));
        let props: Vec<(&str, Option<&str>, &str)> =
            card.properties.iter().map(|p| (p.label.as_str(), p.ty.as_deref(), p.description.as_str())).collect();
        assert_eq!(props, vec![("title", Some("string"), "Shown on top."), ("size?", Some("number"), "")]);
        let shape = tree.planned.nodes.iter().find(|x| x.name == "CardProps").unwrap();
        assert_eq!(shape.technology, None, "the props interface itself is a data shape");
    }

    /// When a test's body names a symbol defined in its own file or in a file it imports, the symbol records that claim as exercising it; a name the test binds itself, a name under three characters, and a file the test does not import record nothing.
    #[test]
    fn a_symbol_records_the_claims_that_exercise_it() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        write(d, "web/package.json", "{\"name\": \"web\"}\n");
        write(d, "web/src/money.ts", "export function add(a: number, b: number) { return a + b; }\nexport function ok() { return 1; }\nexport function total() { return 0; }\n");
        write(d, "web/src/other.ts", "export function add() { return 0; }\n");
        write(
            d,
            "web/src/money.test.ts",
            "import { add, ok } from \"./money\";\nit(\"Adds two amounts\", () => { const total = add(1, 2); ok(); expect(total).toBe(3); });\nit.todo(\"Adds three amounts\", () => { add(1, 2); });\n",
        );
        write(d, "Cargo.toml", "[package]\nname = \"core\"\nversion = \"0.1.0\"\n");
        write(d, "src/lib.rs", "pub mod domain;\n");
        write(
            d,
            "src/domain.rs",
            "pub fn post(n: u32) -> u32 { n }\n#[cfg(test)]\nmod tests {\n    /// Posts an amount\n    #[test]\n    fn posts() { assert_eq!(super::post(1), 1); }\n}\n",
        );
        let tree = derived_tree_uncached(d).unwrap();
        let claim = |desc: &str| tree.claims.iter().find(|c| c.description == desc).unwrap().id.clone();
        let exercised = |m: &scryer_core::ScryModel, file: &str, name: &str| {
            let node = m.nodes.iter().find(|n| n.name == name && m.source_map[&n.id][0].pattern == file).unwrap();
            node.exercised_by.clone()
        };
        let (adds, adds3, posts) = (claim("Adds two amounts"), claim("Adds three amounts"), claim("Posts an amount"));
        let m = &tree.planned;
        assert_eq!(exercised(m, "web/src/money.ts", "add"), vec![adds.clone(), adds3]);
        assert!(exercised(m, "web/src/money.ts", "ok").is_empty(), "a two-letter name is too short to tell");
        assert!(exercised(m, "web/src/money.ts", "total").is_empty(), "the test's own `total` is a local");
        assert!(exercised(m, "web/src/other.ts", "add").is_empty(), "the test does not import other.ts");
        assert_eq!(exercised(m, "src/domain.rs", "post"), vec![posts]);
        assert_eq!(exercised(&tree.committed, "web/src/money.ts", "add"), vec![adds], "a planned claim is in the plan only");
    }

    /// When structure.toml declares persons and externals but relates none of them, each person gets an unlabelled link to the system and the system one to each external; a declared relationship replaces the default.
    #[test]
    fn unrelated_actors_get_default_links() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        write(d, "Cargo.toml", "[package]\nname = \"ledger\"\nversion = \"0.1.0\"\n");
        write(d, "src/lib.rs", "pub fn post() {}\n");
        write(
            d,
            ".scryer/structure.toml",
            "[system]\nname = \"Pay\"\npersons = [\"merchant\", \"auditor\"]\nexternals = [\"bank\", \"db\"]\n\n[relationships]\nauditor = [{ to = \"Pay\", label = \"reviews\" }]\n\"Pay\" = [{ to = \"bank\", label = \"settles with\" }]\n",
        );
        let tree = derived_tree_uncached(d).unwrap();
        let m = &tree.committed;
        let id = |name: &str| m.nodes.iter().find(|n| n.name == name).unwrap().id.clone();
        let link = |a: &str, b: &str| m.links.iter().find(|l| l.src == id(a) && l.dst == id(b)).map(|l| l.label.as_str());
        assert_eq!(link("merchant", "Pay"), Some(""));
        assert_eq!(link("auditor", "Pay"), Some("reviews"));
        assert_eq!(link("Pay", "db"), Some(""));
        assert_eq!(link("Pay", "bank"), Some("settles with"));
        assert_eq!(m.links.iter().filter(|l| l.dst == id("Pay") || l.src == id("Pay")).count(), 4);
    }

    /// When a relationship names a container, the link sits on that container and is lifted to the system for the context map with its label.
    #[test]
    fn a_relationship_to_a_container_lands_on_it_and_lifts_to_the_system() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        write(d, "Cargo.toml", "[workspace]\nmembers = [\"crates/*\"]\n");
        write(d, "crates/api/Cargo.toml", "[package]\nname = \"api\"\nversion = \"0.1.0\"\n");
        write(d, "crates/api/src/lib.rs", "pub fn serve() {}\n");
        write(
            d,
            ".scryer/structure.toml",
            "[system]\nname = \"Pay\"\npersons = [\"merchant\"]\nexternals = [\"bank\"]\n\n[relationships]\nmerchant = [{ to = \"API\", label = \"calls\" }]\n\n[containers.api]\nname = \"API\"\nuses = [{ to = \"bank\", label = \"settles with\" }]\n",
        );
        let tree = derived_tree_uncached(d).unwrap();
        let m = &tree.committed;
        let id = |name: &str| m.nodes.iter().find(|n| n.name == name).unwrap().id.clone();
        let link = |a: &str, b: &str| m.links.iter().find(|l| l.src == id(a) && l.dst == id(b)).map(|l| l.label.as_str());
        assert_eq!(link("merchant", "API"), Some("calls"));
        assert_eq!(link("merchant", "Pay"), Some("calls"));
        assert_eq!(link("API", "bank"), Some("settles with"));
        assert_eq!(link("Pay", "bank"), Some("settles with"));
    }

    /// When a JS/TS package imports `@tauri-apps/api` and a crate of the repo depends on `tauri`, the package links to the crate as invoking its commands.
    #[test]
    fn a_tauri_frontend_links_to_its_host_crate() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        write(d, "package.json", "{\"name\": \"app\", \"dependencies\": {\"@tauri-apps/api\": \"^2\"}}\n");
        write(d, "src/api.ts", "import { invoke } from \"@tauri-apps/api/core\";\nexport function load() { return invoke(\"load\"); }\n");
        write(d, "src-tauri/Cargo.toml", "[package]\nname = \"host\"\nversion = \"0.1.0\"\n[dependencies]\ntauri = \"2\"\n[build-dependencies]\ntauri-build = \"2\"\n");
        write(d, "src-tauri/src/main.rs", "fn main() {}\n");
        write(d, "crates/plugin/Cargo.toml", "[package]\nname = \"plugin\"\nversion = \"0.1.0\"\n[dependencies]\ntauri = \"2\"\n");
        write(d, "crates/plugin/src/lib.rs", "pub fn init() {}\n");
        let tree = derived_tree_uncached(d).unwrap();
        let m = &tree.committed;
        let id = |name: &str| m.nodes.iter().find(|n| n.name == name && n.kind == Kind::Container).unwrap().id.clone();
        let link = |a: &str, b: &str| m.links.iter().find(|l| l.src == id(a) && l.dst == id(b)).map(|l| l.label.as_str());
        assert_eq!(link("app", "host"), Some("invokes commands"));
        assert_eq!(link("app", "plugin"), None, "the app crate, not every crate on tauri");
    }

    /// When a package has an entry point, its components are the directories the entry reaches through imports, and what nothing reaches is listed as the container's tooling, neither a component nor dark.
    #[test]
    fn a_packages_components_are_what_its_entries_reach() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        write(d, "package.json", "{\"name\": \"web\"}\n");
        write(d, "index.html", "<html><body><script type=\"module\" src=\"/src/app/main.tsx\"></script></body></html>\n");
        write(d, "src/app/main.tsx", "import { Page } from \"../pages/Page\";\nexport const root = Page;\n");
        write(d, "src/pages/Page.tsx", "export function Page() { return null; }\n");
        write(d, "src/shared/unused.ts", "export const lost = 1;\n");
        write(d, "scripts/export.mjs", "export function run() {}\n");
        write(d, "vite.config.ts", "export default {};\n");
        let tree = derived_tree_uncached(d).unwrap();
        let m = &tree.planned;
        let components: BTreeSet<&str> = m.nodes.iter().filter(|n| n.kind == Kind::Component).map(|n| n.name.as_str()).collect();
        assert_eq!(components, BTreeSet::from(["app", "pages", "shared"]), "the source root an entry sits in is reached whole");
        let web = m.nodes.iter().find(|n| n.kind == Kind::Container).unwrap();
        assert_eq!(tree.tooling[&web.id], vec!["scripts/export.mjs".to_string(), "vite.config.ts".to_string()]);
        assert!(tree.dark_files.iter().all(|f| !f.starts_with("scripts/")), "{:?}", tree.dark_files);
        assert!(tree.dark_files.contains(&"src/shared/unused.ts".to_string()), "{:?}", tree.dark_files);
    }

    /// When a package declares no entry point and has none by convention, every directory in it stays a component.
    #[test]
    fn without_an_entry_every_directory_is_a_component() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        write(d, "package.json", "{\"name\": \"web\"}\n");
        write(d, "lib/a.ts", "export const a = 1;\n");
        write(d, "scripts/b.mjs", "export const b = 1;\n");
        let tree = derived_tree_uncached(d).unwrap();
        let components: BTreeSet<&str> =
            tree.planned.nodes.iter().filter(|n| n.kind == Kind::Component).map(|n| n.name.as_str()).collect();
        assert_eq!(components, BTreeSet::from(["lib", "scripts"]));
        assert!(tree.tooling.is_empty());
    }

    /// When a crate has lib, bin or example targets, its build script is tooling while its source modules and examples stay components.
    #[test]
    fn a_crates_build_script_is_tooling() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        write(d, "Cargo.toml", "[package]\nname = \"host\"\nversion = \"0.1.0\"\n");
        write(d, "build.rs", "fn main() {}\n");
        write(d, "src/main.rs", "mod shell;\nfn main() {}\n");
        write(d, "src/shell/mod.rs", "pub fn run() {}\n");
        write(d, "examples/smoke.rs", "fn main() {}\n");
        let tree = derived_tree_uncached(d).unwrap();
        let components: BTreeSet<&str> =
            tree.planned.nodes.iter().filter(|n| n.kind == Kind::Component).map(|n| n.name.as_str()).collect();
        assert_eq!(components, BTreeSet::from(["examples", "shell"]));
        assert_eq!(tree.tooling.values().flatten().collect::<Vec<_>>(), vec!["build.rs"]);
    }

    /// When a directory inside a package has an app entry of its own (an index.html or a bundler config), it is a container named after the directory, not a component of the package.
    #[test]
    fn a_nested_app_is_its_own_container() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        write(d, "package.json", "{\"name\": \"web\", \"main\": \"src/index.ts\"}\n");
        write(d, "src/index.ts", "export const x = 1;\n");
        write(d, "harness/index.html", "<script type=\"module\" src=\"./main.tsx\"></script>\n");
        write(d, "harness/main.tsx", "import { x } from \"../src/index\";\nexport const y = x;\n");
        write(d, "harness/vite.config.ts", "export default {};\n");
        write(d, "demo/remotion.config.ts", "export default {};\n");
        write(d, "demo/Root.tsx", "export function Root() { return null; }\n");
        write(d, "tests/fixtures/site/index.html", "<script type=\"module\" src=\"./a.js\"></script>\n");
        write(d, "tests/fixtures/site/a.js", "export const a = 1;\n");
        let tree = derived_tree_uncached(d).unwrap();
        let containers: BTreeSet<&str> =
            tree.planned.nodes.iter().filter(|n| n.kind == Kind::Container).map(|n| n.name.as_str()).collect();
        assert_eq!(containers, BTreeSet::from(["demo", "harness", "web"]));
        let harness = tree.planned.nodes.iter().find(|n| n.name == "harness").unwrap();
        assert_eq!(tree.tooling[&harness.id], vec!["harness/vite.config.ts".to_string()]);
    }

    /// When structure.toml declares a container for a directory no manifest makes one, the directory becomes that container.
    #[test]
    fn structure_toml_promotes_a_directory_to_a_container() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        write(d, "package.json", "{\"name\": \"web\"}\n");
        write(d, "src/a.ts", "export const a = 1;\n");
        write(d, "preview/server.mjs", "export function serve() {}\n");
        write(d, ".scryer/structure.toml", "[containers.preview]\nname = \"Preview Server\"\n");
        let tree = derived_tree_uncached(d).unwrap();
        let m = &tree.planned;
        let preview = m.nodes.iter().find(|n| n.name == "Preview Server").unwrap();
        assert_eq!(preview.kind, Kind::Container);
        let serve = m.nodes.iter().find(|n| n.name == "serve").unwrap();
        let parent = m.nodes.iter().find(|n| Some(n.id.as_str()) == serve.parent_id.as_deref()).unwrap();
        assert_eq!(parent.parent_id.as_deref(), Some(preview.id.as_str()));
    }

    /// When a file is reached only by its colocated test's import, or is a
    /// barrel whose re-exports are reached, it is not dark; a file nothing
    /// reaches still is.
    #[test]
    fn colocated_tests_and_barrels_reach_code() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        write(d, "web/package.json", "{\"name\": \"web\"}\n");
        write(d, "web/src/shared/api.ts", "export type Fetcher = () => void;\nexport function fetchWith(f: Fetcher) { f(); }\n");
        write(
            d,
            "web/src/shared/api.test.ts",
            "import { fetchWith, type Fetcher } from \"./api\";\ndescribe(\"api\", () => {\n  it(\"Calls the fetcher\", () => { const f: Fetcher = () => {}; fetchWith(f); });\n});\n",
        );
        write(d, "web/src/shared/cache.ts", "export function keep() { return 1; }\n");
        write(d, "web/src/shared/index.ts", "export { fetchWith, type Fetcher } from \"./api\";\nexport { keep } from \"./cache\";\n");
        write(d, "Cargo.toml", "[package]\nname = \"core\"\nversion = \"0.1.0\"\n");
        write(d, "src/lib.rs", "pub mod domain;\n");
        write(d, "src/domain/mod.rs", "pub mod money;\n");
        write(
            d,
            "src/domain/money.rs",
            "pub fn add(a: u32, b: u32) -> u32 { a + b }\n#[cfg(test)]\nmod tests {\n    /// Adds two amounts\n    #[test]\n    fn adds() { assert_eq!(super::add(1, 2), 3); }\n}\n",
        );
        let tree = derived_tree_uncached(d).unwrap();
        let dark: BTreeSet<&str> = tree.dark_files.iter().map(String::as_str).collect();
        for lit in ["web/src/shared/api.ts", "web/src/shared/index.ts", "src/lib.rs", "src/domain/mod.rs"] {
            assert!(!dark.contains(lit), "{lit} is reached: {dark:?}");
        }
        assert!(dark.contains("web/src/shared/cache.ts"), "{dark:?}");
    }

    /// When a documented, tested file becomes a sub-component, it takes its
    /// directory's layer, and its imports are held to the style.
    #[test]
    fn a_sub_component_sits_on_its_directorys_layer() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        write(d, "Cargo.toml", "[package]\nname = \"ledger\"\nversion = \"0.1.0\"\n");
        write(d, "src/lib.rs", "pub mod domain;\npub mod infrastructure;\n");
        write(d, "src/domain/mod.rs", "pub mod money;\n");
        write(
            d,
            "src/domain/money.rs",
            "//! Amounts in minor units.\npub fn add(a: u32) -> u32 { crate::infrastructure::db::open(); a }\n#[cfg(test)]\nmod tests {\n    /// Adds an amount\n    #[test]\n    fn adds() { assert_eq!(super::add(1), 1); }\n}\n",
        );
        write(d, "src/infrastructure/mod.rs", "pub mod db;\n");
        write(d, "src/infrastructure/db.rs", "pub fn open() {}\n");
        write(d, ".scryer/structure.toml", "[containers.\"\"]\nstyle = \"library\"\n");
        let tree = derived_tree_uncached(d).unwrap();
        let money = tree.planned.nodes.iter().find(|n| n.name == "money").unwrap();
        assert_eq!(money.kind, Kind::Component);
        assert_eq!(money.layer.as_deref(), Some("domain"));
        assert!(tree.style_violations.iter().all(|v| v.kind != "layerless"), "{:?}", tree.style_violations);
        assert!(
            tree.style_violations.iter().any(|v| v.detail.contains("domain") && v.detail.contains("infrastructure")),
            "{:?}",
            tree.style_violations
        );
    }

    /// When two files of one layer directory are each a sub-component, an
    /// import between them stays inside one slice of the layer, so a style
    /// that isolates its slices reports nothing.
    #[test]
    fn sub_components_of_one_directory_are_one_slice() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        write(d, "web/package.json", "{\"name\": \"web\"}\n");
        write(d, "web/src/entities/payment.ts", "/** A payment. */\nexport type Payment = { amount: number };\nexport function total(p: Payment) { return p.amount; }\n");
        write(d, "web/src/entities/payment.test.ts", "import { total } from \"./payment\";\nit(\"Totals a payment\", () => { total({ amount: 1 }); });\n");
        write(d, "web/src/entities/refund.ts", "/** A refund. */\nimport { total, type Payment } from \"./payment\";\nexport function remaining(p: Payment) { return total(p); }\n");
        write(d, "web/src/entities/refund.test.ts", "import { remaining } from \"./refund\";\nit(\"Leaves what was not refunded\", () => { remaining({ amount: 1 }); });\n");
        write(d, ".scryer/structure.toml", "[containers.web]\nstyle = \"feature-sliced\"\n");
        let tree = derived_tree_uncached(d).unwrap();
        let named = |n: &str| tree.planned.nodes.iter().find(|x| x.name == n).unwrap();
        assert_eq!(named("refund").layer.as_deref(), Some("entities"));
        assert!(tree.style_violations.is_empty(), "{:?}", tree.style_violations);
    }

    /// When a forbidden link comes from an import outside any definition (a
    /// module-scope `use` behind an `impl Trait for T`), its finding names
    /// the importing file, not the component's first file.
    #[test]
    fn a_forbidden_link_names_the_importing_file() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        write(d, "Cargo.toml", "[package]\nname = \"ledger\"\nversion = \"0.1.0\"\n");
        write(d, "src/lib.rs", "pub mod application;\npub mod domain;\n");
        write(d, "src/application/mod.rs", "pub mod store;\n");
        write(d, "src/application/store.rs", "pub trait Store { fn put(&self); }\n");
        write(d, "src/domain/mod.rs", "pub mod account;\npub mod memory;\n");
        write(d, "src/domain/account.rs", "pub struct Account;\n");
        write(d, "src/domain/memory.rs", "use crate::application::store::Store;\npub struct Memory;\nimpl Store for Memory {\n    fn put(&self) {}\n}\n");
        write(d, ".scryer/structure.toml", "[containers.\"\"]\nstyle = \"hexagonal\"\n");
        let tree = derived_tree_uncached(d).unwrap();
        let link: Vec<_> = tree.style_violations.iter().filter(|v| v.kind == "forbidden_link").collect();
        assert_eq!(link.len(), 1, "{:?}", tree.style_violations);
        assert_eq!(link[0].file, "src/domain/memory.rs");
    }

    /// When no file changed, the cached tree is returned; touching a file
    /// derives again.
    #[test]
    fn caches_on_file_mtimes() {
        let dir = cargo_workspace();
        let first = derived_tree(dir.path()).unwrap();
        assert!(dir.path().join(".scryer/.derived_tree.json").exists());
        let again = derived_tree(dir.path()).unwrap();
        assert_eq!(first.fingerprint, again.fingerprint);

        std::thread::sleep(std::time::Duration::from_millis(20));
        write(dir.path(), "crates/core/src/store/mod.rs", "//! Persists text durably.\npub struct Store;\n");
        let changed = derived_tree(dir.path()).unwrap();
        assert_ne!(first.fingerprint, changed.fingerprint);
        assert!(changed.planned.nodes.iter().any(|n| n.description.as_deref() == Some("Persists text durably.")));
        // And the core read path sees the same tree.
        let r = ModelRef::ProjectLocal(dir.path().to_path_buf());
        assert_eq!(read_derived_tree_at(&r).unwrap().fingerprint, changed.fingerprint);
    }

    /// Ad-hoc: summarise the derived tree of any repo without writing to it [manual]
    ///
    /// `REPO=/path cargo test -p scryer-extract dump_derived -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn dump_derived() {
        let repo = PathBuf::from(std::env::var("REPO").expect("set REPO"));
        let started = std::time::Instant::now();
        let tree = derived_tree_uncached(&repo).expect("derivation");
        let m = &tree.planned;
        let count = |k: Kind| m.nodes.iter().filter(|n| n.kind == k).count();
        let by_level = |l: ClaimLevel| tree.claims.iter().filter(|c| c.level == l).count();
        let described = m.nodes.iter().filter(|n| n.description.is_some()).count();
        let lifted_docs = crate::infrastructure::walk::project_files(&repo)
            .iter()
            .filter(|rel| scan::is_product_code(rel))
            .filter_map(|rel| {
                let src = std::fs::read_to_string(repo.join(rel)).ok()?;
                claims::lift_file(rel, &src, &mut tree_sitter::Parser::new()).module_description
            })
            .count();
        eprintln!("derived in {:?}", started.elapsed());
        eprintln!(
            "containers {}, components {}, symbols {}, links {}",
            count(Kind::Container),
            count(Kind::Component),
            count(Kind::Symbol),
            m.links.len()
        );
        eprintln!(
            "claims {} (system {}, container {}, component {}), planned {}, with concerns {}",
            tree.claims.len(),
            by_level(ClaimLevel::System),
            by_level(ClaimLevel::Container),
            by_level(ClaimLevel::Component),
            tree.claims.iter().filter(|c| c.planned).count(),
            tree.claims.iter().filter(|c| !c.concerns.is_empty()).count(),
        );
        eprintln!("files with a module doc {lifted_docs}, nodes with a module description {described}");
        for n in m.nodes.iter().filter(|n| n.description.is_some() && std::env::var("DUMP_DESC").is_ok()) {
            eprintln!("    {:?} {}: {}", n.kind, n.name, n.description.as_deref().unwrap_or_default());
        }
        for c in m.nodes.iter().filter(|n| n.kind == Kind::Container) {
            let comps: Vec<&scryer_core::Node> = m
                .nodes
                .iter()
                .filter(|n| n.kind == Kind::Component && n.parent_id.as_deref() == Some(c.id.as_str()))
                .collect();
            let own = c.responsibilities.len();
            // Unit claims sit on components and their sub-components.
            let mut below: BTreeSet<&str> = comps.iter().map(|n| n.id.as_str()).collect();
            for n in m.nodes.iter().filter(|n| n.kind == Kind::Component) {
                if n.parent_id.as_deref().is_some_and(|p| below.contains(p)) {
                    below.insert(n.id.as_str());
                }
            }
            let unit: usize = m
                .nodes
                .iter()
                .filter(|n| below.contains(n.id.as_str()))
                .map(|n| n.responsibilities.len())
                .sum();
            eprintln!(
                "  {} — {} components, {} container claims, {} unit claims{}",
                c.name,
                comps.len(),
                own,
                unit,
                c.description.as_deref().map(|d| format!(" — {d}")).unwrap_or_default()
            );
        }
        let sys = m.nodes.iter().find(|n| n.kind == Kind::System && n.external.is_none()).unwrap();
        eprintln!("  system claims on {}: {}", sys.name, sys.responsibilities.len());
        if let Ok(path) = std::env::var("DUMP_NODE") {
            let ids: BTreeSet<&str> = m.nodes.iter().filter(|n| n.name == path).map(|n| n.id.as_str()).collect();
            let mut files: BTreeMap<&str, usize> = BTreeMap::new();
            for c in tree.claims.iter().filter(|c| ids.contains(c.node_id.as_str())) {
                *files.entry(c.file.as_str()).or_default() += 1;
            }
            eprintln!("  claims on {path}: {files:?}");
        }
        for c in tree.claims.iter().take(8) {
            eprintln!("  e.g. [{:?}{}] {} — {}:{}", c.level, if c.planned { ", planned" } else { "" }, c.description, c.file, c.line);
        }
    }

    /// When a crate has a lib.rs or main.rs, makes it the crate's root rather than a component, with its claims on the crate and its imports kept in the graph.
    #[test]
    fn a_crates_root_file_belongs_to_the_crate() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        write(d, "Cargo.toml", "[workspace]\nmembers = [\"crates/*\"]\n");
        write(d, "crates/notes/Cargo.toml", "[package]\nname = \"notes\"\nversion = \"0.1.0\"\n");
        write(
            d,
            "crates/notes/src/lib.rs",
            "//! Keeps a writer's notes.\npub mod core;\npub fn open() -> core::Note { core::Note }\n\n#[cfg(test)]\nmod tests {\n    /// Opens an empty note\n    #[test]\n    fn opens() {}\n}\n",
        );
        write(d, "crates/notes/src/main.rs", "fn main() { notes::open(); }\n");
        write(d, "crates/notes/src/core/mod.rs", "//! The note itself.\npub struct Note;\n");
        write(d, ".scryer/structure.toml", "[containers.notes]\nstyle = \"core-shell\"\n");
        let tree = derived_tree(d).unwrap();
        let m = &tree.planned;
        let names: Vec<&str> = m.nodes.iter().filter(|n| n.kind == Kind::Component).map(|n| n.name.as_str()).collect();
        assert_eq!(names, ["core"], "no lib or main component");
        let crate_node = m.nodes.iter().find(|n| n.kind == Kind::Container).unwrap();
        assert_eq!(crate_node.description.as_deref(), Some("Keeps a writer's notes."));
        let open = m.nodes.iter().find(|n| n.name == "open").unwrap();
        assert_eq!(open.parent_id.as_deref(), Some(crate_node.id.as_str()), "a root definition sits on the crate");
        assert_eq!(tree.claims[0].node_id, crate_node.id, "a root's unit test sits on the crate");
        assert!(tree.style_violations.is_empty(), "{:?}", tree.style_violations);
    }

    /// When a .NET test project mirrors a product project, its unit tests
    /// nest under the product component they exercise and the test project
    /// is not a container of its own.
    #[test]
    fn dotnet_test_projects_attach_to_what_they_test() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        write(d, "src/Server/Server.csproj", "<Project Sdk=\"Microsoft.NET.Sdk\"></Project>");
        write(d, "src/Server/domain/Dock.cs", "namespace Server.Domain;\npublic class Dock { public bool Accept() => true; }\n");
        write(
            d,
            "tests/Server.Tests/Server.Tests.csproj",
            "<Project Sdk=\"Microsoft.NET.Sdk\"><ItemGroup><ProjectReference Include=\"../../src/Server/Server.csproj\" /></ItemGroup></Project>",
        );
        write(
            d,
            "tests/Server.Tests/DockTests.cs",
            "using Server.Domain;\nnamespace Server.Tests;\npublic class DockTests {\n    /// <summary>Accepts a rig in range</summary>\n    [Fact]\n    public void Accepts() { new Dock().Accept(); }\n}\n",
        );
        let tree = derived_tree(d).unwrap();
        let m = &tree.planned;
        let containers: Vec<&str> = m.nodes.iter().filter(|n| n.kind == Kind::Container).map(|n| n.name.as_str()).collect();
        assert_eq!(containers, vec!["Server"]);
        let claim = &tree.claims[0];
        assert_eq!(claim.description, "Accepts a rig in range");
        assert_eq!(claim.level, ClaimLevel::Component);
        let home = m.nodes.iter().find(|n| n.id == claim.node_id).unwrap();
        assert_eq!(home.name, "domain");
    }
}
