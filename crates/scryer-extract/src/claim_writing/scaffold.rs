//! A template scaffolded into a project: packaging, entry files carrying
//! each module doc, and each claim as a todo test.
//!
//! The scaffold is the plan, not the implementation: an agent fills the
//! todos and the claims verify it. It writes in the destination language's
//! conventions, so a template stays language-neutral; Rust is the one
//! written so far.
//!
//! The template's contract (the claims its source's users made about it) is
//! never written: no container here is known to be its user. It is handed
//! back for the agent to place as todo tests where this project uses the
//! template, which is how the wiring gets verified.
//!
//! It never overwrites. Every file it would create is checked before
//! anything is written, and one that exists refuses the whole scaffold. A
//! container the destination already has (its manifest is there) keeps its
//! own manifest, doc and `structure.toml` section; the template's components
//! join it, declared in its entry file, and its claims are appended to the
//! test files they belong in.

use crate::claim_text::insert_stub;
use crate::derivation::STRUCTURE_FILE;
use crate::template::{ContractClaim, Template, TemplateClaim, TemplateComponent, TemplateContainer, TEMPLATE_VERSION};
use scryer_core::derived_tree::ClaimLevel;
use std::collections::BTreeMap;
use std::path::Path;

/// What a scaffold wrote, by project-relative path, and what it left for
/// the developer.
#[derive(Debug, Default, Clone, PartialEq, serde::Serialize)]
pub struct Scaffolded {
    pub written: Vec<String>,
    pub edited: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
    /// The template's contract, for the agent to place.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub contract: Vec<ContractClaim>,
}

/// The files a scaffold will write, held until every one is known to be
/// safe to write.
struct Plan<'p> {
    project: &'p Path,
    /// Path → (contents, whether it is a new file).
    files: BTreeMap<String, (String, bool)>,
    conflicts: Vec<String>,
    notes: Vec<String>,
}

impl Plan<'_> {
    fn on_disk(&self, rel: &str) -> Option<String> {
        std::fs::read_to_string(self.project.join(rel)).ok()
    }

    fn exists(&self, rel: &str) -> bool {
        self.files.contains_key(rel) || self.project.join(rel).exists()
    }

    fn read(&self, rel: &str) -> Option<String> {
        self.files.get(rel).map(|(t, _)| t.clone()).or_else(|| self.on_disk(rel))
    }

    /// A file the scaffold creates; one already there is a conflict.
    fn create(&mut self, rel: String, text: String) {
        if self.exists(&rel) {
            self.conflicts.push(rel);
        } else {
            self.files.insert(rel, (text, true));
        }
    }

    /// A file the scaffold creates or adds to.
    fn put(&mut self, rel: String, text: String) {
        let new = self.files.get(&rel).map_or(!self.project.join(&rel).exists(), |(_, n)| *n);
        self.files.insert(rel, (text, new));
    }

    fn commit(self) -> Result<Scaffolded, String> {
        if !self.conflicts.is_empty() {
            return Err(format!("would overwrite {}; nothing was written", self.conflicts.join(", ")));
        }
        let mut out = Scaffolded { notes: self.notes, ..Default::default() };
        for (rel, (text, new)) in self.files {
            let path = self.project.join(&rel);
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
            }
            std::fs::write(&path, text).map_err(|e| format!("{rel}: {e}"))?;
            if new { out.written.push(rel) } else { out.edited.push(rel) }
        }
        std::fs::create_dir_all(self.project.join(".scryer")).map_err(|e| e.to_string())?;
        Ok(out)
    }
}

/// Scaffold `template` into `project`, all of it or nothing.
pub fn scaffold(project: &Path, template: &Template) -> Result<Scaffolded, String> {
    if template.scryer_template != TEMPLATE_VERSION {
        return Err(format!(
            "template format {} is not the one this scryer reads ({TEMPLATE_VERSION})",
            template.scryer_template
        ));
    }
    let mut plan = Plan { project, files: BTreeMap::new(), conflicts: Vec::new(), notes: Vec::new() };
    for c in &template.containers {
        match c.language.as_str() {
            "rust" => rust::container(&mut plan, c)?,
            other => return Err(format!("no scaffolder writes {other} yet; Rust is the one built")),
        }
    }
    rust::workspace(&mut plan, template);
    declare(&mut plan, template);
    plan.notes.push(
        "the claims and module docs keep the template's wording: reword them for this project's domain, \
         and rename the modules to match, before building"
            .into(),
    );
    if !template.contract.is_empty() {
        plan.notes.push(
            "the contract lists what the source's other code claimed where it used this: write each that applies \
             as a todo test where this project uses the template, and wire it in until they pass"
                .into(),
        );
    }
    let mut out = plan.commit()?;
    out.contract = template.contract.clone();
    Ok(out)
}

fn join(dir: &str, rest: &str) -> String {
    if dir.is_empty() { rest.to_string() } else { format!("{dir}/{rest}") }
}

fn quote(s: &str) -> String {
    toml::Value::String(s.to_string()).to_string()
}

/// Each new container's display name, style and layers in `structure.toml`.
/// A container it already declares keeps its section as written.
fn declare(plan: &mut Plan, template: &Template) {
    let mut text = plan.read(STRUCTURE_FILE).unwrap_or_default();
    let before = text.clone();
    for c in &template.containers {
        let headers = [format!("[containers.\"{}\"]", c.key), format!("[containers.{}]", c.key)];
        if text.lines().any(|l| headers.iter().any(|h| l.trim() == h)) {
            continue;
        }
        let mut fields = Vec::new();
        if let Some(n) = &c.name {
            fields.push(format!("name = {}", quote(n)));
        }
        if let Some(s) = &c.style {
            fields.push(format!("style = {}", quote(s)));
        }
        if !c.layers.is_empty() {
            let layers: Vec<String> = c.layers.iter().map(|l| quote(l)).collect();
            fields.push(format!("layers = [{}]", layers.join(", ")));
        }
        if fields.is_empty() {
            continue;
        }
        let sep = if text.is_empty() || text.ends_with("\n\n") { "" } else if text.ends_with('\n') { "\n" } else { "\n\n" };
        text = format!("{text}{sep}{}\n{}\n", headers[0], fields.join("\n"));
    }
    if text != before {
        plan.put(STRUCTURE_FILE.to_string(), text);
    }
}

/// `source` with each claim appended as a todo test, its decisions after
/// the claim in the test's doc.
fn with_claims<'c>(rel: &str, mut source: Option<String>, claims: impl Iterator<Item = &'c TemplateClaim>) -> Result<Option<String>, String> {
    for claim in claims {
        let sentence = claim.sentence();
        let mut text = insert_stub(rel, source.as_deref(), &sentence, None)?;
        if let Some(d) = claim.decisions.as_deref().filter(|d| !d.trim().is_empty()) {
            text = rust::with_test_decisions(&text, &sentence, d);
        }
        source = Some(text);
    }
    Ok(source)
}

mod rust {
    //! The Rust scaffolder: a Cargo package per container, a module per
    //! component (`name.rs`, or `name/mod.rs` when it holds others), unit
    //! claims in the module's own test module and integration claims in the
    //! package's `tests/claims.rs`.

    use super::*;

    /// The `//!` doc carrying a description, its layer and its decisions.
    fn module_doc(description: Option<&str>, layer: Option<&str>, decisions: Option<&str>) -> String {
        let head = [description.map(str::to_string), layer.map(|l| format!("[{l}]"))]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" ");
        if head.is_empty() {
            return String::new();
        }
        let mut lines = vec![format!("//! {head}")];
        if let Some(d) = decisions.filter(|d| !d.trim().is_empty()) {
            lines.push("//!".into());
            lines.extend(d.trim().lines().map(|l| if l.trim().is_empty() { "//!".into() } else { format!("//! {}", l.trim_end()) }));
        }
        lines.join("\n") + "\n"
    }

    fn ident(name: &str) -> String {
        crate::claim_text::snake_name(name)
    }

    /// `source` with `pub mod name;` declared, after the module doc and the
    /// modules it already declares.
    fn declare_mod(source: &str, name: &str) -> String {
        let line = format!("pub mod {name};");
        let declared = source.lines().any(|l| {
            let l = l.trim();
            l.strip_prefix("pub ").unwrap_or(l).strip_prefix("mod ").and_then(|r| r.strip_suffix(';')) == Some(name)
        });
        if declared {
            return source.to_string();
        }
        let lines: Vec<&str> = source.lines().collect();
        let is_mod = |l: &str| {
            let l = l.trim_start_matches("pub(crate) ").trim_start_matches("pub ");
            l.starts_with("mod ") && l.ends_with(';')
        };
        let at = lines
            .iter()
            .rposition(|l| is_mod(l))
            .map(|i| i + 1)
            .unwrap_or_else(|| lines.iter().take_while(|l| l.starts_with("//!")).count());
        let mut out: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
        out.insert(at, line);
        out.join("\n") + "\n"
    }

    /// The test `insert_stub` just wrote for `sentence`, with `decisions`
    /// after the claim in its doc.
    pub(super) fn with_test_decisions(text: &str, sentence: &str, decisions: &str) -> String {
        let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
        if let Some(i) = lines.iter().rposition(|l| l.trim() == format!("/// {sentence}")) {
            let indent: String = lines[i].chars().take_while(|c| c.is_whitespace()).collect();
            let mut doc = vec![format!("{indent}///")];
            doc.extend(decisions.trim().lines().map(|l| {
                if l.trim().is_empty() { format!("{indent}///") } else { format!("{indent}/// {}", l.trim_end()) }
            }));
            lines.splice(i + 1..i + 1, doc);
        }
        lines.join("\n") + "\n"
    }

    pub(super) fn container(plan: &mut Plan, c: &TemplateContainer) -> Result<(), String> {
        let manifest = join(&c.key, "Cargo.toml");
        let src = join(&c.key, "src");
        let unit = c.claims.iter().filter(|k| k.level == ClaimLevel::Component);
        let integration = c.claims.iter().filter(|k| k.level == ClaimLevel::Container);
        let joining = plan.exists(&manifest);
        if joining {
            let entry = [join(&src, "lib.rs"), join(&src, "main.rs")]
                .into_iter()
                .find(|f| plan.exists(f))
                .ok_or_else(|| format!("{manifest} has no src/lib.rs or src/main.rs for the components to join"))?;
            let mut text = plan.read(&entry).unwrap_or_default();
            for comp in &c.components {
                text = declare_mod(&text, &ident(&comp.name));
            }
            if let Some(t) = with_claims(&entry, Some(text.clone()), unit)? {
                text = t;
            }
            if plan.read(&entry).as_deref() != Some(&text) {
                plan.put(entry, text);
            }
        } else {
            plan.create(
                manifest,
                format!("[package]\nname = {}\nversion = \"0.1.0\"\nedition = \"2021\"\n", quote(&c.package)),
            );
            let entry = join(&src, "lib.rs");
            let mut text = module_doc(c.description.as_deref(), None, c.decisions.as_deref());
            for comp in &c.components {
                text = declare_mod(&text, &ident(&comp.name));
            }
            let text = with_claims(&entry, Some(text.clone()), unit)?.unwrap_or(text);
            plan.create(entry, text);
        }
        let tests = join(&c.key, "tests/claims.rs");
        let had = plan.read(&tests);
        if let Some(text) = with_claims(&tests, had.clone(), integration)? {
            if had.as_deref() != Some(&text) {
                plan.put(tests, text);
            }
        }
        for comp in &c.components {
            component(plan, &src, comp, joining)?;
        }
        Ok(())
    }

    /// The component's module in `dir`: joined when `joining` a module the
    /// destination already has, written when it does not.
    fn component(plan: &mut Plan, dir: &str, comp: &TemplateComponent, joining: bool) -> Result<(), String> {
        let name = ident(&comp.name);
        let file = join(dir, &format!("{name}.rs"));
        let mod_rs = join(dir, &format!("{name}/mod.rs"));
        let existing = [&mod_rs, &file].into_iter().find(|f| joining && plan.exists(f)).cloned();
        let joined = existing.is_some();
        let children = join(dir, &name);
        let unit = comp.claims.iter().filter(|k| k.level == ClaimLevel::Component);
        match existing {
            Some(entry) => {
                let mut text = plan.read(&entry).unwrap_or_default();
                for sub in &comp.components {
                    text = declare_mod(&text, &ident(&sub.name));
                }
                let text = with_claims(&entry, Some(text.clone()), unit)?.unwrap_or(text);
                if plan.read(&entry).as_deref() != Some(&text) {
                    plan.put(entry, text);
                }
            }
            None => {
                let entry = if comp.components.is_empty() { file } else { mod_rs };
                let mut text = module_doc(comp.description.as_deref(), comp.layer.as_deref(), comp.decisions.as_deref());
                for sub in &comp.components {
                    text = declare_mod(&text, &ident(&sub.name));
                }
                let text = with_claims(&entry, Some(text.clone()), unit)?.unwrap_or(text);
                plan.create(entry, text);
            }
        }
        if comp.claims.iter().any(|k| k.level != ClaimLevel::Component) {
            plan.notes.push(format!("{}: a component's claims are unit claims; the others were left out", comp.name));
        }
        for sub in &comp.components {
            component(plan, &children, sub, joined)?;
        }
        Ok(())
    }

    /// A root `Cargo.toml` workspace holding the new packages, when there is
    /// none; when there is one, a note for each package it may not list.
    pub(super) fn workspace(plan: &mut Plan, template: &Template) {
        let members: Vec<&str> = template
            .containers
            .iter()
            .filter(|c| c.language == "rust" && !c.key.is_empty())
            .map(|c| c.key.as_str())
            .collect();
        if members.is_empty() {
            return;
        }
        match plan.read("Cargo.toml") {
            None => {
                let list: Vec<String> = members.iter().map(|m| quote(m)).collect();
                plan.put("Cargo.toml".into(), format!("[workspace]\nmembers = [{}]\nresolver = \"2\"\n", list.join(", ")));
            }
            Some(root) => {
                for m in members {
                    if !root.contains(m) {
                        plan.notes.push(format!("add {m} to the workspace members in Cargo.toml if no glob there covers it"));
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::derived_tree::{derived_tree_uncached, read_structure};
    use crate::template::template_of;
    use scryer_core::Kind;

    fn write(dir: &Path, rel: &str, text: &str) {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn read(dir: &Path, rel: &str) -> String {
        std::fs::read_to_string(dir.join(rel)).unwrap()
    }

    /// A hexagonal ledger: a domain component holding a nested one, an
    /// infrastructure component, unit and integration claims, decisions on
    /// modules and on a test.
    fn source() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        write(d, "Cargo.toml", "[workspace]\nmembers = [\"crates/*\"]\n");
        write(d, "crates/ledger/Cargo.toml", "[package]\nname = \"ledger\"\nversion = \"0.1.0\"\n");
        write(
            d,
            "crates/ledger/src/lib.rs",
            "//! Keeps accounts' balances.\n//!\n//! Amounts are minor units.\n//!\n//! - never floats\npub mod account;\npub mod store;\n\
             #[cfg(test)]\nmod tests {\n    /// Opens with no accounts\n    #[test]\n    fn opens() {}\n}\n",
        );
        write(
            d,
            "crates/ledger/src/account/mod.rs",
            "//! An account's balance, from its entries. [domain]\n//!\n//! Derived, never stored.\npub mod entry;\n\
             pub fn tally(e: &[i64]) -> i64 { e.iter().sum() }\n\
             #[cfg(test)]\nmod tests {\n    /// Sums the entries\n    #[test]\n    fn sums() { assert_eq!(super::tally(&[1, 2]), 3); }\n\n\
             \x20   /// If an entry overflows, then refuses the posting [safety]\n    ///\n    /// A wrapped balance is worse than none.\n    #[test]\n    #[ignore]\n    fn overflow() { todo!() }\n}\n",
        );
        write(
            d,
            "crates/ledger/src/account/entry.rs",
            "//! One posting to an account. [domain]\npub struct Entry;\n#[cfg(test)]\nmod tests {\n    /// Rejects a zero amount\n    #[test]\n    fn zero() {}\n}\n",
        );
        write(d, "crates/ledger/src/store.rs", "//! Persists entries. [infrastructure]\npub fn put() {}\n");
        write(d, "crates/ledger/tests/claims.rs", "/// When an entry is posted, persists it [persistence]\n#[test]\nfn posts() {}\n");
        write(
            d,
            "crates/web/Cargo.toml",
            "[package]\nname = \"web\"\nversion = \"0.1.0\"\n[dependencies]\nledger = { path = \"../ledger\" }\n",
        );
        write(
            d,
            "crates/web/src/lib.rs",
            "//! Serves pages.\nuse ledger::account::tally;\npub fn show() -> i64 { tally(&[1]) }\n\
             #[cfg(test)]\nmod tests {\n    /// Shows the account's balance\n    #[test]\n    fn shows() { assert_eq!(super::show(), 1); }\n}\n",
        );
        write(
            d,
            ".scryer/structure.toml",
            "[containers.\"crates/ledger\"]\nname = \"Ledger\"\nstyle = \"hexagonal\"\nlayers = [\"domain\", \"infrastructure\"]\n",
        );
        dir
    }

    fn template_at(dir: &Path, root: &str) -> Template {
        let tree = derived_tree_uncached(dir).unwrap();
        template_of(&tree, &read_structure(dir).unwrap(), root).unwrap()
    }

    fn ledger() -> Template {
        let src = source();
        template_at(src.path(), "crates/ledger")
    }

    /// When a template is scaffolded into an empty directory, derives the template it was made from, all but its contract
    #[test]
    fn round_trips_through_an_empty_directory() {
        let t = ledger();
        assert!(!t.contract.is_empty(), "the source has a user of the ledger");
        let dest = tempfile::tempdir().unwrap();
        scaffold(dest.path(), &t).unwrap();
        assert_eq!(template_at(dest.path(), "crates/ledger"), Template { contract: Vec::new(), ..t });
        let tree = derived_tree_uncached(dest.path()).unwrap();
        assert!(tree.style_violations.is_empty(), "{:?}", tree.style_violations);
        assert!(tree.claims.iter().all(|c| c.planned), "every claim is work to do");
    }

    /// Writes each component's entry file with its module doc and layer
    #[test]
    fn writes_entry_files_with_their_doc_and_layer() {
        let dest = tempfile::tempdir().unwrap();
        let d = dest.path();
        let out = scaffold(d, &ledger()).unwrap();
        assert!(out.written.contains(&"crates/ledger/Cargo.toml".to_string()));
        assert!(read(d, "Cargo.toml").contains("members = [\"crates/ledger\"]"));
        let account = read(d, "crates/ledger/src/account/mod.rs");
        assert!(account.starts_with("//! An account's balance, from its entries. [domain]\n//!\n//! Derived, never stored.\npub mod entry;\n"), "{account}");
        assert!(read(d, "crates/ledger/src/store.rs").starts_with("//! Persists entries. [infrastructure]\n"));
        assert!(read(d, "crates/ledger/src/lib.rs").contains("pub mod account;\npub mod store;\n"));
        let structure = read(d, STRUCTURE_FILE);
        assert!(structure.contains("[containers.\"crates/ledger\"]\nname = \"Ledger\"\nstyle = \"hexagonal\"\nlayers = [\"domain\", \"infrastructure\"]\n"), "{structure}");
    }

    /// Writes each claim as a todo test where tests of its level go
    #[test]
    fn writes_claims_as_todo_tests() {
        let dest = tempfile::tempdir().unwrap();
        let d = dest.path();
        scaffold(d, &ledger()).unwrap();
        let tree = derived_tree_uncached(d).unwrap();
        for c in &tree.claims {
            let target = match c.level {
                ClaimLevel::Component => tree.doc_files[&c.node_id].clone(),
                _ => "crates/ledger/tests/claims.rs".to_string(),
            };
            assert_eq!(c.file, target, "{}", c.description);
        }
        let account = read(d, "crates/ledger/src/account/mod.rs");
        assert!(account.contains("    /// If an entry overflows, then refuses the posting [safety]\n    ///\n    /// A wrapped balance is worse than none.\n    #[test]\n    #[ignore]\n"), "{account}");
        assert!(read(d, "crates/ledger/tests/claims.rs").contains("/// When an entry is posted, persists it [persistence]\n#[test]\n#[ignore]\n"));
    }

    /// When the template's container is already declared, adds its components to it
    #[test]
    fn adds_components_to_a_declared_container() {
        let src = source();
        let tree = derived_tree_uncached(src.path()).unwrap();
        let entry = tree.planned.nodes.iter().find(|n| n.name == "entry" && n.kind == Kind::Component).unwrap();
        let t = template_at(src.path(), &entry.id);

        let dest = tempfile::tempdir().unwrap();
        let d = dest.path();
        write(d, "Cargo.toml", "[workspace]\nmembers = [\"crates/*\"]\n");
        write(d, "crates/ledger/Cargo.toml", "[package]\nname = \"ledger\"\nversion = \"0.1.0\"\n");
        write(d, "crates/ledger/src/lib.rs", "//! Our own ledger.\npub mod account;\n");
        write(d, "crates/ledger/src/account.rs", "//! Our accounts. [domain]\npub struct Account;\n");
        write(d, ".scryer/structure.toml", "[containers.\"crates/ledger\"]\nstyle = \"hexagonal\"\n");

        let out = scaffold(d, &t).unwrap();
        assert_eq!(out.written, ["crates/ledger/src/account/entry.rs"]);
        assert_eq!(out.edited, ["crates/ledger/src/account.rs"]);
        assert_eq!(read(d, "crates/ledger/src/lib.rs"), "//! Our own ledger.\npub mod account;\n");
        assert_eq!(read(d, "crates/ledger/src/account.rs"), "//! Our accounts. [domain]\npub mod entry;\npub struct Account;\n");
        assert_eq!(read(d, STRUCTURE_FILE), "[containers.\"crates/ledger\"]\nstyle = \"hexagonal\"\n");
        let tree = derived_tree_uncached(d).unwrap();
        let landed = tree.planned.nodes.iter().find(|n| n.name == "entry" && n.kind == Kind::Component).unwrap();
        assert_eq!(landed.layer.as_deref(), Some("domain"));
        assert!(tree.claims.iter().any(|c| c.description == "Rejects a zero amount" && c.node_id == landed.id && c.planned));
    }

    /// If a file it would write already exists, then refuses and writes nothing
    #[test]
    fn refuses_to_overwrite() {
        let dest = tempfile::tempdir().unwrap();
        let d = dest.path();
        write(d, "crates/ledger/src/store.rs", "pub fn mine() {}\n");
        let err = scaffold(d, &ledger()).unwrap_err();
        assert!(err.contains("crates/ledger/src/store.rs"), "{err}");
        assert_eq!(read(d, "crates/ledger/src/store.rs"), "pub fn mine() {}\n");
        assert!(!d.join("crates/ledger/src/lib.rs").exists() && !d.join("Cargo.toml").exists());
    }

    /// If no scaffolder writes the template's language yet, then refuses
    #[test]
    fn refuses_an_unbuilt_language() {
        let mut t = ledger();
        t.containers[0].language = "python".into();
        let dest = tempfile::tempdir().unwrap();
        let err = scaffold(dest.path(), &t).unwrap_err();
        assert!(err.contains("python"), "{err}");
        assert!(std::fs::read_dir(dest.path()).unwrap().next().is_none());
    }

    /// Lists the contract's claims for the agent to place, writing none of them
    #[test]
    fn lists_the_contract_for_the_agent() {
        let t = ledger();
        let dest = tempfile::tempdir().unwrap();
        let d = dest.path();
        let out = scaffold(d, &t).unwrap();
        assert_eq!(out.contract, t.contract);
        assert_eq!(out.contract[0].claim.text, "Shows the account's balance");
        assert!(out.notes.iter().any(|n| n.contains("contract")), "{:?}", out.notes);
        let tree = derived_tree_uncached(d).unwrap();
        assert!(tree.claims.iter().all(|c| c.description != "Shows the account's balance"));
        assert!(!d.join("crates/web").exists());
    }
}
