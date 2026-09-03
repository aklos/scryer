//! Code-time enforcement of the model's styles.
//!
//! [`crate::validate`] checks the links the agent DECLARED; this module
//! checks the imports the code ACTUALLY has. The build's dependency graph is
//! already resolved to node pairs by [`crate::build_edges::derive_graph`];
//! one more pass maps each end to `(component, layer)` and applies the
//! governing style's table. Everything here is deterministic and derived;
//! nothing is stored. It is the language-agnostic twin of dependency-cruiser,
//! import-linter and ArchUnit, driven by the model instead of a per-tool
//! config.

use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use crate::build_edges::{DerivedGraph, ExternalImport};
use crate::ownership::BoundaryOwnership;
use crate::style::{self, StyleDef, Styles};
use crate::{Node, ScryModel};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ViolationKind {
    /// An import whose `(layer(src), layer(dst))` pair the matrix forbids, or
    /// one that enters a container from outside on a non-inbound layer.
    LayerViolation,
    /// A same-layer import between two components with no declared link, or
    /// one that bypasses the target component's public surface.
    IsolationViolation,
    /// A file importing a package its layer bans (a domain importing React).
    ExternalViolation,
    /// A file whose path says one layer while its component says another.
    Misplaced,
}

impl ViolationKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ViolationKind::LayerViolation => "layer_violation",
            ViolationKind::IsolationViolation => "isolation_violation",
            ViolationKind::ExternalViolation => "external_violation",
            ViolationKind::Misplaced => "misplaced",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StyleViolation {
    pub kind: ViolationKind,
    /// The component the violation is charged to: the importer, or the owner
    /// of the misplaced / banned-import file.
    pub node: String,
    /// The other component involved, for the two edge-shaped kinds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub other: Option<String>,
    /// The file the fix happens in.
    pub file: String,
    /// The container whose style was applied.
    pub container: String,
    /// One line a reader can act on without opening anything else.
    pub detail: String,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StyleReport {
    /// Sorted by (kind, node, file, other) — stable across runs.
    pub violations: Vec<StyleViolation>,
    pub layer_violations: usize,
    pub isolation_violations: usize,
    pub external_violations: usize,
    pub misplaced: usize,
}

impl StyleReport {
    pub fn total(&self) -> usize {
        self.violations.len()
    }

    /// Violations charged to nodes in `scope` (a subtree's id set).
    pub fn scoped(&self, scope: &HashSet<&str>) -> StyleReport {
        let violations: Vec<StyleViolation> = self
            .violations
            .iter()
            .filter(|v| scope.contains(v.node.as_str()))
            .cloned()
            .collect();
        StyleReport::from_violations(violations)
    }

    fn from_violations(mut violations: Vec<StyleViolation>) -> StyleReport {
        violations.sort();
        violations.dedup();
        let count = |k: ViolationKind| violations.iter().filter(|v| v.kind == k).count();
        StyleReport {
            layer_violations: count(ViolationKind::LayerViolation),
            isolation_violations: count(ViolationKind::IsolationViolation),
            external_violations: count(ViolationKind::ExternalViolation),
            misplaced: count(ViolationKind::Misplaced),
            violations,
        }
    }
}

/// Which component a project file belongs to, from the finest evidence the
/// model carries: a sourceMap anchor into the file wins (its host's
/// component), then the most specific boundary glob (its owner's component).
/// Files owned only at container level belong to no component and are skipped.
struct FileIndex<'a> {
    model: &'a ScryModel,
    by_id: HashMap<&'a str, &'a Node>,
    anchored: HashMap<&'a str, &'a str>,
    ownership: BoundaryOwnership,
    /// Component id → files the model ties to it (anchors + owned inventory).
    component_files: BTreeMap<&'a str, BTreeSet<String>>,
}

impl<'a> FileIndex<'a> {
    fn new(model: &'a ScryModel, files: Option<&BTreeSet<String>>) -> Self {
        let by_id: HashMap<&str, &Node> = model.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
        let component_id = |id: &str| -> Option<&'a str> {
            style::layer_component(model, id).map(|c| c.id.as_str())
        };
        let mut anchored: HashMap<&str, &str> = HashMap::new();
        let mut component_files: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
        let mut resp_host: HashMap<&str, &str> = HashMap::new();
        for n in &model.nodes {
            for r in &n.responsibilities {
                resp_host.insert(r.id.as_str(), n.id.as_str());
            }
        }
        for (key, locs) in &model.source_map {
            let host = resp_host.get(key.as_str()).copied().unwrap_or(key.as_str());
            let Some(comp) = component_id(host) else { continue };
            for loc in locs {
                anchored.entry(loc.pattern.as_str()).or_insert(comp);
                component_files.entry(comp).or_default().insert(loc.pattern.clone());
            }
        }
        let ownership = BoundaryOwnership::new(model);
        if let Some(files) = files {
            for (owner, owned) in ownership.owned_by(files.iter().map(String::as_str)) {
                let Some(comp) = component_id(&owner) else { continue };
                component_files.entry(comp).or_default().extend(owned);
            }
        }
        Self { model, by_id, anchored, ownership, component_files }
    }

    fn component_of_file(&self, file: &str) -> Option<&'a Node> {
        if let Some(&comp) = self.anchored.get(file) {
            return self.by_id.get(comp).copied();
        }
        // Deepest boundary owner among the nodes whose winning glob claims it.
        let owners = self.ownership.owned_by(std::iter::once(file));
        let mut best: Option<&'a Node> = None;
        for owner in owners.keys() {
            let Some(comp) = style::layer_component(self.model, owner) else { continue };
            best = Some(match best {
                Some(b) if b.id <= comp.id => b,
                _ => comp,
            });
        }
        best
    }

    fn files_of(&self, comp: &str) -> Option<&BTreeSet<String>> {
        self.component_files.get(comp)
    }
}

fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn ban_matches(package: &str, ban: &str) -> bool {
    package == ban || package.strip_prefix(ban).is_some_and(|rest| rest.starts_with('/'))
}

/// Run the code-time checks. `derived` comes from the build's dependency
/// cache, `externals` from the same build, `files` is the project inventory
/// (without it the misplaced check covers anchored files only and the
/// public-surface check is skipped).
pub fn check_code(
    model: &ScryModel,
    styles: &Styles,
    derived: &DerivedGraph,
    externals: &[ExternalImport],
    files: Option<&BTreeSet<String>>,
) -> StyleReport {
    let index = FileIndex::new(model, files);
    let mut out: Vec<StyleViolation> = Vec::new();
    let governing = |id: &str| -> Option<&StyleDef> {
        styles.get(style::governing_style(model, id)?)
    };

    // --- import edges ------------------------------------------------------------
    for e in &derived.resolved_edges {
        let (Some(src_comp), Some(dst_comp)) = (
            style::layer_component(model, &e.src_node),
            style::layer_component(model, &e.dst_node),
        ) else {
            continue;
        };
        let (Some(sl), Some(dl)) = (src_comp.layer.as_deref(), dst_comp.layer.as_deref()) else {
            continue;
        };
        let (Some(src_c), Some(dst_c)) = (
            style::container_of(model, &src_comp.id),
            style::container_of(model, &dst_comp.id),
        ) else {
            continue;
        };
        let charge = |kind, other: &Node, container: &Node, detail: String| StyleViolation {
            kind,
            node: src_comp.id.clone(),
            other: Some(other.id.clone()),
            file: e.src_file.clone(),
            container: container.id.clone(),
            detail,
        };

        if src_c.id != dst_c.id {
            let Some(def) = governing(&dst_comp.id) else { continue };
            if !def.inbound.is_empty() && !def.is_inbound(dl) {
                out.push(charge(
                    ViolationKind::LayerViolation,
                    dst_comp,
                    dst_c,
                    format!(
                        "{} `{}` reaches into container '{}' at `{}` ({}), which is on layer \
                         '{dl}' — from outside, enter through {}",
                        e.src_file, e.src_symbol, dst_c.name, e.dst_symbol, e.dst_file,
                        def.inbound.join(" or ")
                    ),
                ));
            }
            continue;
        }

        let Some(def) = governing(&src_comp.id) else { continue };
        if !def.may_depend(sl, dl) {
            let allowed = def.allowed(sl);
            out.push(charge(
                ViolationKind::LayerViolation,
                dst_comp,
                src_c,
                format!(
                    "{} `{}` ({sl}) imports `{}` from {} ({dl}) — in style '{}' {sl} may depend on {}",
                    e.src_file, e.src_symbol, e.dst_symbol, e.dst_file, def.name,
                    if allowed.is_empty() { "nothing".to_string() } else { allowed.join(", ") }
                ),
            ));
            continue;
        }
        if sl != dl || src_comp.id == dst_comp.id {
            continue;
        }
        // Same layer, different components: needs a declared link, and goes
        // through the sibling's public surface when it has one.
        let declared = model
            .links
            .iter()
            .any(|l| l.src == src_comp.id && l.dst == dst_comp.id);
        if !declared {
            out.push(charge(
                ViolationKind::IsolationViolation,
                dst_comp,
                src_c,
                format!(
                    "{} imports `{}` from sibling {dl} component '{}' with no declared link — \
                     declare '{}' → '{}' (kind: uses) or move the code",
                    e.src_file, e.dst_symbol, dst_comp.name, src_comp.name, dst_comp.name
                ),
            ));
            continue;
        }
        if !def.public_surface.is_empty() && !def.public_surface.iter().any(|p| p == basename(&e.dst_file)) {
            let entry = index.files_of(&dst_comp.id).and_then(|fs| {
                fs.iter().find(|f| def.public_surface.iter().any(|p| p == basename(f)))
            });
            if let Some(entry) = entry {
                out.push(charge(
                    ViolationKind::IsolationViolation,
                    dst_comp,
                    src_c,
                    format!(
                        "{} imports `{}` from {} inside sibling component '{}', bypassing its \
                         public surface — import it from {} instead",
                        e.src_file, e.dst_symbol, e.dst_file, dst_comp.name, entry
                    ),
                ));
            }
        }
    }

    // --- banned packages ---------------------------------------------------------
    for imp in externals {
        let Some(comp) = index.component_of_file(&imp.file) else { continue };
        let Some(layer) = comp.layer.as_deref() else { continue };
        let Some(def) = governing(&comp.id) else { continue };
        let Some(bans) = def.external_bans.get(layer) else { continue };
        let Some(ban) = bans.iter().find(|b| ban_matches(&imp.package, b)) else { continue };
        let Some(container) = style::container_of(model, &comp.id) else { continue };
        out.push(StyleViolation {
            kind: ViolationKind::ExternalViolation,
            node: comp.id.clone(),
            other: None,
            file: imp.file.clone(),
            container: container.id.clone(),
            detail: format!(
                "{} ('{}', {layer}) imports `{}` — in style '{}' the {layer} layer never depends \
                 on {ban}; move the code that needs it to a layer that may",
                imp.file, comp.name, imp.package, def.name
            ),
        });
    }

    // --- path convention ---------------------------------------------------------
    for (comp_id, comp_files) in &index.component_files {
        let Some(comp) = index.by_id.get(comp_id).copied() else { continue };
        let Some(layer) = comp.layer.as_deref() else { continue };
        let Some(def) = governing(comp_id) else { continue };
        let Some(container) = style::container_of(model, comp_id) else { continue };
        let prefix = style::container_prefix(model, &container.id);
        for file in comp_files {
            let rel = prefix
                .as_deref()
                .and_then(|p| file.strip_prefix(p))
                .unwrap_or(file.as_str());
            let Some(path_layer) = def.layer_of_path(rel) else { continue };
            if path_layer == layer {
                continue;
            }
            out.push(StyleViolation {
                kind: ViolationKind::Misplaced,
                node: comp.id.clone(),
                other: None,
                file: file.clone(),
                container: container.id.clone(),
                detail: format!(
                    "{} sits on a {path_layer} path but belongs to '{}' ({layer}) — move it \
                     under {} or re-layer the component",
                    file,
                    comp.name,
                    def.layer_dir(layer).map(|d| format!("{d}/")).unwrap_or_else(|| layer.to_string())
                ),
            });
        }
    }

    StyleReport::from_violations(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build_edges::ResolvedEdge;
    use crate::{Link, LinkKind};

    fn node(v: serde_json::Value) -> Node {
        serde_json::from_value(v).unwrap()
    }

    fn anchor(model: &mut ScryModel, key: &str, file: &str, symbol: &str) {
        model.source_map.insert(
            key.into(),
            vec![serde_json::from_value(serde_json::json!({ "pattern": file, "symbol": symbol })).unwrap()],
        );
    }

    /// One hexagonal service: presentation Http, application Checkout and
    /// Refunds, domain Orders, infrastructure Postgres — each with one anchored
    /// symbol — under `svc/`.
    fn hex_service() -> ScryModel {
        let mut m = ScryModel::new();
        m.nodes = vec![
            node(serde_json::json!({ "id": "sys", "kind": "system", "name": "S" })),
            node(serde_json::json!({ "id": "svc", "kind": "container", "name": "Svc", "parentId": "sys", "style": "hexagonal" })),
            node(serde_json::json!({ "id": "ui", "kind": "container", "name": "Ui", "parentId": "sys", "style": "feature-sliced" })),
        ];
        let comps = [
            ("pres", "Http", "presentation", "svc/presentation/http.rs", "serve"),
            ("app", "Checkout", "application", "svc/application/checkout.rs", "checkout"),
            ("app2", "Refunds", "application", "svc/application/refunds.rs", "refund"),
            ("dom", "Orders", "domain", "svc/domain/order.rs", "Order"),
            ("infra", "Postgres", "infrastructure", "svc/infrastructure/pg.rs", "PgOrders"),
        ];
        for (id, name, layer, file, sym) in comps {
            m.nodes.push(node(serde_json::json!({
                "id": id, "kind": "component", "name": name, "parentId": "svc", "layer": layer,
                "responsibilities": [{ "id": format!("r-{id}"), "statement": "does x" }]
            })));
            anchor(&mut m, &format!("r-{id}"), file, sym);
        }
        m.nodes.push(node(serde_json::json!({
            "id": "page", "kind": "component", "name": "Page", "parentId": "ui", "layer": "pages",
            "responsibilities": [{ "id": "r-page", "statement": "shows" }]
        })));
        anchor(&mut m, "r-page", "ui/pages/checkout.tsx", "CheckoutPage");
        m.boundaries.insert(
            "svc".into(),
            vec![crate::Source { pattern: "svc/**/*".into(), comment: None }],
        );
        m
    }

    fn edge(src: (&str, &str, &str), dst: (&str, &str, &str)) -> ResolvedEdge {
        ResolvedEdge {
            src_node: src.0.into(), src_symbol: src.1.into(), src_file: src.2.into(),
            dst_node: dst.0.into(), dst_symbol: dst.1.into(), dst_file: dst.2.into(),
            count: 1,
        }
    }

    fn report(m: &ScryModel, edges: Vec<ResolvedEdge>, externals: Vec<(&str, &str)>) -> StyleReport {
        let derived = DerivedGraph { resolved_edges: edges, ..Default::default() };
        let externals: Vec<ExternalImport> = externals
            .into_iter()
            .map(|(f, p)| ExternalImport { file: f.into(), package: p.into() })
            .collect();
        check_code(m, &Styles::builtin(), &derived, &externals, None)
    }

    #[test]
    fn imports_against_the_matrix_are_layer_violations() {
        let m = hex_service();
        let r = report(
            &m,
            vec![
                // legal: presentation → application, application → domain, infra → application
                edge(("pres", "serve", "svc/presentation/http.rs"), ("app", "checkout", "svc/application/checkout.rs")),
                edge(("app", "checkout", "svc/application/checkout.rs"), ("dom", "Order", "svc/domain/order.rs")),
                edge(("infra", "PgOrders", "svc/infrastructure/pg.rs"), ("app", "checkout", "svc/application/checkout.rs")),
                // illegal: domain → infrastructure, presentation → infrastructure
                edge(("dom", "Order", "svc/domain/order.rs"), ("infra", "PgOrders", "svc/infrastructure/pg.rs")),
                edge(("pres", "serve", "svc/presentation/http.rs"), ("infra", "PgOrders", "svc/infrastructure/pg.rs")),
            ],
            vec![],
        );
        assert_eq!(r.layer_violations, 2, "{:#?}", r.violations);
        assert_eq!(r.total(), 2);
        let v = &r.violations[0];
        assert_eq!(v.node, "dom");
        assert_eq!(v.other.as_deref(), Some("infra"));
        assert!(v.detail.contains("(domain) imports `PgOrders`") && v.detail.contains("domain may depend on domain"), "{}", v.detail);
    }

    #[test]
    fn entering_a_container_below_its_inbound_layer_is_a_layer_violation() {
        let m = hex_service();
        let r = report(
            &m,
            vec![
                edge(("page", "CheckoutPage", "ui/pages/checkout.tsx"), ("app", "checkout", "svc/application/checkout.rs")),
                edge(("page", "CheckoutPage", "ui/pages/checkout.tsx"), ("dom", "Order", "svc/domain/order.rs")),
            ],
            vec![],
        );
        assert_eq!(r.layer_violations, 1, "{:#?}", r.violations);
        assert!(r.violations[0].detail.contains("reaches into container 'Svc' at `Order`"), "{}", r.violations[0].detail);
        assert!(r.violations[0].detail.contains("enter through presentation or application"));
    }

    #[test]
    fn same_layer_imports_need_a_declared_link_and_the_public_surface() {
        let mut m = hex_service();
        let e = edge(("app", "checkout", "svc/application/checkout.rs"), ("app2", "refund", "svc/application/refunds.rs"));
        let r = report(&m, vec![e.clone()], vec![]);
        assert_eq!(r.isolation_violations, 1, "{:#?}", r.violations);
        assert!(r.violations[0].detail.contains("with no declared link"), "{}", r.violations[0].detail);

        // Declared → silent (Refunds has no public-surface file, so nothing to bypass).
        m.links.push(Link {
            id: "l".into(), src: "app".into(), dst: "app2".into(),
            label: String::new(), method: None, kind: Some(LinkKind::Uses),
        });
        assert_eq!(report(&m, vec![e.clone()], vec![]).total(), 0);

        // Give Refunds a mod.rs entry point: importing its inner file bypasses it.
        let files: BTreeSet<String> = ["svc/application/refunds/mod.rs", "svc/application/refunds.rs"]
            .into_iter().map(String::from).collect();
        m.boundaries.insert(
            "app2".into(),
            vec![crate::Source { pattern: "svc/application/refunds/**/*".into(), comment: None }],
        );
        let derived = DerivedGraph { resolved_edges: vec![e], ..Default::default() };
        let r = check_code(&m, &Styles::builtin(), &derived, &[], Some(&files));
        assert_eq!(r.isolation_violations, 1, "{:#?}", r.violations);
        assert!(r.violations[0].detail.contains("bypassing its public surface"), "{}", r.violations[0].detail);
        assert!(r.violations[0].detail.contains("svc/application/refunds/mod.rs"));
    }

    #[test]
    fn banned_packages_are_external_violations() {
        let m = hex_service();
        let r = report(
            &m,
            vec![],
            vec![
                ("svc/domain/order.rs", "sqlx"),
                ("svc/domain/order.rs", "serde"),
                ("svc/infrastructure/pg.rs", "sqlx"),
                ("ui/pages/checkout.tsx", "react"),
            ],
        );
        assert_eq!(r.external_violations, 1, "{:#?}", r.violations);
        let v = &r.violations[0];
        assert_eq!((v.node.as_str(), v.file.as_str()), ("dom", "svc/domain/order.rs"));
        assert!(v.detail.contains("imports `sqlx`") && v.detail.contains("domain layer never depends on sqlx"), "{}", v.detail);
    }

    #[test]
    fn a_file_on_another_layers_path_is_misplaced() {
        let mut m = hex_service();
        // Orders (domain) gets a second anchored file that lives under infrastructure/.
        m.nodes.iter_mut().find(|n| n.id == "dom").unwrap().responsibilities.push(
            serde_json::from_value(serde_json::json!({ "id": "r-dom2", "statement": "prices" })).unwrap(),
        );
        anchor(&mut m, "r-dom2", "svc/infrastructure/pricing.rs", "price");
        let r = report(&m, vec![], vec![]);
        assert_eq!(r.misplaced, 1, "{:#?}", r.violations);
        let v = &r.violations[0];
        assert_eq!(v.file, "svc/infrastructure/pricing.rs");
        assert!(v.detail.contains("sits on a infrastructure path but belongs to 'Orders' (domain)"), "{}", v.detail);
        assert!(v.detail.contains("move it under domain/"));
    }

    /// The container's own root directory never reads as a layer: a container
    /// living at `api/` is not "presentation" for every file under it.
    #[test]
    fn the_container_prefix_is_not_read_as_a_layer_dir() {
        let mut m = ScryModel::new();
        m.nodes = vec![
            node(serde_json::json!({ "id": "sys", "kind": "system", "name": "S" })),
            node(serde_json::json!({ "id": "api", "kind": "container", "name": "Api", "parentId": "sys", "style": "hexagonal" })),
            node(serde_json::json!({ "id": "dom", "kind": "component", "name": "Orders", "parentId": "api", "layer": "domain",
                                     "responsibilities": [{ "id": "r1", "statement": "x" }] })),
        ];
        m.boundaries.insert("api".into(), vec![crate::Source { pattern: "api/**/*".into(), comment: None }]);
        anchor(&mut m, "r1", "api/domain/order.rs", "Order");
        assert_eq!(report(&m, vec![], vec![]).total(), 0);
        anchor(&mut m, "r1", "api/order.rs", "Order");
        assert_eq!(report(&m, vec![], vec![]).total(), 0, "a path that says nothing is fine");
    }

    #[test]
    fn scoped_keeps_only_violations_charged_inside_the_subtree() {
        let m = hex_service();
        let r = report(
            &m,
            vec![edge(("dom", "Order", "svc/domain/order.rs"), ("infra", "PgOrders", "svc/infrastructure/pg.rs"))],
            vec![("ui/pages/checkout.tsx", "react")],
        );
        assert_eq!(r.total(), 1);
        let scope: HashSet<&str> = ["dom"].into_iter().collect();
        assert_eq!(r.scoped(&scope).total(), 1);
        let scope: HashSet<&str> = ["page"].into_iter().collect();
        assert_eq!(r.scoped(&scope).total(), 0);
    }
}
