//! Architectural styles — the model's horizontal axis.
//!
//! C4 gives the model a vertical axis (system → container → component →
//! symbol) but no rule for what a component *is* inside its container or which
//! components may depend on which. A **style** supplies that: a fixed list of
//! layer names, a legality matrix over them, an isolation rule for siblings on
//! the same layer, and the path convention that puts each layer in one
//! predictable place. Every container declares a style; every component under
//! it carries one layer from that style's list. Symbols inherit their
//! component's layer.
//!
//! Style and layer are the node-level twins of EARS and concerns: fixed
//! vocabularies that let a reader scan without parsing. Unlike a concern, a
//! layer is required and must come from the style's list, because the map and
//! the checks depend on it.
//!
//! The four built-ins (`hexagonal`, `feature-sliced`, `core-shell`,
//! `pipeline`) are ordinary [`StyleDef`] values; a project may add its own as
//! `.scryer/styles/<name>.json` with the same shape. The engine never
//! special-cases a built-in.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

use crate::{Kind, Node, ScryModel};

/// One layer of a style: its name (the tag components carry) and a one-line
/// description of what belongs there — the only prose the agent ever sees
/// about the style.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LayerDef {
    pub name: String,
    #[serde(default)]
    pub description: String,
}

/// Whether two components on the SAME layer may import each other.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum Isolation {
    /// Siblings on a layer are isolated (Feature-Sliced Design's slices).
    Strict,
    /// Siblings on a layer may import each other freely (hexagonal, Nx).
    Inclusive,
}

/// How layer maps onto a path under a node's boundary glob. Either form is
/// accepted; the first directory name listed is the one placement suggests.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PathConvention {
    /// layer → directory names that mark it (`domain/`, `entities/`).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub dirs: BTreeMap<String, Vec<String>>,
    /// layer → file-name infixes/suffixes that mark it (`.port.ts`, `stg_`).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub markers: BTreeMap<String, Vec<String>>,
}

/// The established drawing a style renders in — the "you are here" shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum Drawing {
    /// Cockburn's hexagon: domain centre, application ring, ports on the edge, adapters outside.
    Hexagon,
    /// Stacked rows, one per layer, first layer on top.
    Rows,
    /// Concentric regions, last layer innermost.
    Rings,
    /// Left to right, one column per layer.
    Columns,
}

/// A complete style: the bundle the checks, the placement and the renderer all
/// read from. Serialized as JSON so a project can author its own.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StyleDef {
    /// Kebab-case identifier, the value a container's `style` field carries.
    pub name: String,
    /// One line on what kind of thing this style fits.
    #[serde(default)]
    pub description: String,
    /// Layers in display order (outermost first for layered styles, first stage
    /// first for pipelines). The matrix, not this order, decides legality.
    pub layers: Vec<LayerDef>,
    /// layer → the layers it may depend on (itself included where same-layer
    /// imports are legal). Missing key = may depend on nothing.
    pub matrix: BTreeMap<String, Vec<String>>,
    pub isolation: Isolation,
    /// Layers a cross-container link may land on when it enters this container,
    /// outermost first. Components on every inbound layer but the first must
    /// be reached by something (a use case nobody drives is dead).
    #[serde(default)]
    pub inbound: Vec<String>,
    /// Layers that may reach OUT of the container (talk to other containers,
    /// databases, external systems): the driven side. A dependency on the
    /// outside from any other layer is a violation.
    #[serde(default)]
    pub outbound: Vec<String>,
    /// File basenames that count as a module's public entry point.
    #[serde(default)]
    pub public_surface: Vec<String>,
    /// layer → packages it may not import (a domain that imports React).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub external_bans: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub path: PathConvention,
    pub drawing: Drawing,
}

impl StyleDef {
    pub fn has_layer(&self, layer: &str) -> bool {
        self.layers.iter().any(|l| l.name == layer)
    }

    pub fn layer_names(&self) -> Vec<&str> {
        self.layers.iter().map(|l| l.name.as_str()).collect()
    }

    /// The layers `from` may depend on, in matrix order. Empty if unknown.
    pub fn allowed(&self, from: &str) -> &[String] {
        self.matrix.get(from).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Is an import / link from a `from`-layer node into a `to`-layer node legal?
    pub fn may_depend(&self, from: &str, to: &str) -> bool {
        self.allowed(from).iter().any(|l| l == to)
    }

    /// Is `layer` one a cross-container link may enter this container on?
    pub fn is_inbound(&self, layer: &str) -> bool {
        self.inbound.iter().any(|l| l == layer)
    }

    /// The directory name placement suggests for `layer`, if the convention has one.
    pub fn layer_dir(&self, layer: &str) -> Option<&str> {
        self.path.dirs.get(layer).and_then(|d| d.first()).map(String::as_str)
    }

    /// The layer a project-relative path belongs to by the convention alone —
    /// its innermost matching directory segment or file marker. `None` when the
    /// path says nothing.
    pub fn layer_of_path(&self, path: &str) -> Option<&str> {
        let segments: Vec<&str> = path.split('/').collect();
        let (file, dirs) = segments.split_last()?;
        // Innermost directory wins so `src/app/domain/x.rs` reads as domain.
        for seg in dirs.iter().rev() {
            for l in &self.layers {
                if self
                    .path
                    .dirs
                    .get(&l.name)
                    .is_some_and(|names| names.iter().any(|n| n == seg))
                {
                    return Some(l.name.as_str());
                }
            }
        }
        for l in &self.layers {
            if self
                .path
                .markers
                .get(&l.name)
                .is_some_and(|ms| ms.iter().any(|m| file.contains(m.as_str())))
            {
                return Some(l.name.as_str());
            }
        }
        None
    }
}

/// The set of styles a project can use: the built-ins plus anything under
/// `.scryer/styles/*.json`. A project file with a built-in's name replaces it.
#[derive(Debug, Clone, Default)]
pub struct Styles {
    defs: BTreeMap<String, StyleDef>,
}

impl Styles {
    /// The four built-ins only.
    pub fn builtin() -> Self {
        let mut defs = BTreeMap::new();
        for s in [hexagonal(), feature_sliced(), core_shell(), pipeline()] {
            defs.insert(s.name.clone(), s);
        }
        Self { defs }
    }

    /// Built-ins plus the project's own `.scryer/styles/<name>.json` files.
    /// Unreadable files are skipped, never fatal — a bad custom style must not
    /// take the model down with it.
    pub fn load(project: &Path) -> Self {
        let mut styles = Self::builtin();
        let dir = project.join(".scryer").join("styles");
        let Ok(entries) = std::fs::read_dir(&dir) else { return styles };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else { continue };
            let Ok(def) = serde_json::from_str::<StyleDef>(&text) else { continue };
            styles.defs.insert(def.name.clone(), def);
        }
        styles
    }

    pub fn get(&self, name: &str) -> Option<&StyleDef> {
        self.defs.get(name)
    }

    pub fn names(&self) -> Vec<&str> {
        self.defs.keys().map(String::as_str).collect()
    }

    pub fn iter(&self) -> impl Iterator<Item = &StyleDef> {
        self.defs.values()
    }

    pub fn insert(&mut self, def: StyleDef) {
        self.defs.insert(def.name.clone(), def);
    }
}

// --- Resolution against a model ---------------------------------------------

/// The style name governing `node_id`: the nearest ancestor-or-self at
/// container level or below that declares one. A component may override its
/// container. `None` above container level or when nothing declares a style.
pub fn governing_style<'a>(model: &'a ScryModel, node_id: &str) -> Option<&'a str> {
    let mut cur = model.nodes.iter().find(|n| n.id == node_id);
    let mut hops = 0;
    while let Some(n) = cur {
        if matches!(n.kind, Kind::Container | Kind::Component) {
            if let Some(s) = n.style.as_deref() {
                return Some(s);
            }
        }
        if n.kind == Kind::Container || hops > 8 {
            return None;
        }
        hops += 1;
        cur = n.parent_id.as_deref().and_then(|p| model.nodes.iter().find(|m| m.id == p));
    }
    None
}

/// The component that carries `node_id`'s layer: the node itself when it is a
/// component, its parent when it is a symbol. `None` for anything else.
pub fn layer_component<'a>(model: &'a ScryModel, node_id: &str) -> Option<&'a Node> {
    let n = model.nodes.iter().find(|n| n.id == node_id)?;
    match n.kind {
        Kind::Component => Some(n),
        Kind::Symbol => {
            let p = n.parent_id.as_deref()?;
            model.nodes.iter().find(|m| m.id == p && m.kind == Kind::Component)
        }
        _ => None,
    }
}

/// The layer tag on `node_id`: a component's own, a symbol's inherited from its
/// component. `None` when unset or above component level.
pub fn layer_of<'a>(model: &'a ScryModel, node_id: &str) -> Option<&'a str> {
    layer_component(model, node_id)?.layer.as_deref()
}

/// The container `node_id` sits in (itself when it is a container).
pub fn container_of<'a>(model: &'a ScryModel, node_id: &str) -> Option<&'a Node> {
    let mut cur = model.nodes.iter().find(|n| n.id == node_id);
    let mut hops = 0;
    while let Some(n) = cur {
        if n.kind == Kind::Container {
            return Some(n);
        }
        if hops > 8 {
            return None;
        }
        hops += 1;
        cur = n.parent_id.as_deref().and_then(|p| model.nodes.iter().find(|m| m.id == p));
    }
    None
}

/// The one line an agent needs about the file at hand: its layer, what that
/// layer may import, and where files of that layer live. Computed, never
/// stored; the style table itself is never put in front of the agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Placement {
    pub style: String,
    pub layer: String,
    /// Layers this layer may depend on.
    pub may_import: Vec<String>,
    /// Project-relative directory the convention puts this layer in, when the
    /// container's boundary and the style's convention give one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dir: Option<String>,
}

impl Placement {
    /// `layer: application · may import: application, domain · path: crates/x/src/application/`
    pub fn line(&self) -> String {
        let mut s = format!(
            "layer: {} · may import: {}",
            self.layer,
            if self.may_import.is_empty() { "nothing".to_string() } else { self.may_import.join(", ") }
        );
        if let Some(d) = &self.dir {
            s.push_str(&format!(" · path: {d}"));
        }
        s
    }
}

/// The container's boundary directory (`crates/x/` for `crates/x/**/*`), so
/// a path convention is read and written relative to the container.
pub fn container_prefix(model: &ScryModel, container: &str) -> Option<String> {
    let sources = model.boundaries.get(container)?;
    let pattern = &sources.first()?.pattern;
    let cut = pattern.find(|c: char| c == '*' || c == '?' || c == '{').unwrap_or(pattern.len());
    let prefix = pattern[..cut].trim_end_matches('/');
    (!prefix.is_empty()).then(|| format!("{prefix}/"))
}

/// Where files of `node_id`'s layer belong: the container's boundary prefix
/// (plus `src/` when the project has one there — the Rust and most TS
/// layouts), then the layer's directory. `None` without a boundary or a
/// convention.
pub fn expected_dir(model: &ScryModel, def: &StyleDef, project: &Path, node_id: &str) -> Option<String> {
    let layer = layer_of(model, node_id)?;
    let dir = def.layer_dir(layer)?;
    let container = container_of(model, node_id)?;
    let prefix = container_prefix(model, &container.id)?;
    let with_src = format!("{prefix}src/");
    let base = if project.join(&with_src).is_dir() { with_src } else { prefix };
    Some(format!("{base}{dir}/"))
}

/// The placement line for `node_id` (a component or symbol), or `None` when
/// it has no layer or its style is unknown.
pub fn placement(model: &ScryModel, styles: &Styles, project: &Path, node_id: &str) -> Option<Placement> {
    let layer = layer_of(model, node_id)?;
    let def = styles.get(governing_style(model, node_id)?)?;
    Some(Placement {
        style: def.name.clone(),
        layer: layer.to_string(),
        may_import: def.allowed(layer).to_vec(),
        dir: expected_dir(model, def, project, node_id),
    })
}

/// One symbol a scaffold asks the agent to define.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ScaffoldSymbol {
    pub node_id: String,
    pub name: String,
    /// `type` when the symbol declares a data shape, else `behavior`.
    pub kind: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub responsibilities: Vec<String>,
    /// `label: description` per declared field.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub properties: Vec<String>,
    /// Already anchored somewhere — define it only if it is not there.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub anchored: bool,
}

/// The language-neutral skeleton for one component: where its file(s) go,
/// which layer it is, what it may import, and the symbols to define. The
/// agent materialises it in the project's own language and idiom; scryer
/// never renders code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ScaffoldManifest {
    pub node_id: String,
    pub name: String,
    pub style: String,
    pub layer: String,
    /// Directory the layer's files belong in (project-relative), when the
    /// convention gives one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dir: Option<String>,
    /// Suggested file stem (snake_case of the name); add the language's extension.
    pub basename: String,
    /// The only layers this file may import from.
    pub may_import: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub responsibilities: Vec<String>,
    pub symbols: Vec<ScaffoldSymbol>,
    /// Files the model already anchors into this component.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub existing_files: Vec<String>,
}

fn snake_case(name: &str) -> String {
    let mut out = String::new();
    let mut prev_lower = false;
    for c in name.chars() {
        if c.is_alphanumeric() {
            if c.is_uppercase() && prev_lower {
                out.push('_');
            }
            out.extend(c.to_lowercase());
            prev_lower = c.is_lowercase() || c.is_numeric();
        } else if !out.ends_with('_') && !out.is_empty() {
            out.push('_');
            prev_lower = false;
        }
    }
    out.trim_end_matches('_').to_string()
}

fn strip_markup(s: &str) -> String {
    s.replace("**", "")
}

/// The manifest for one component (`None` when the node is not a component,
/// has no layer, or its style is unknown).
pub fn scaffold_manifest(
    model: &ScryModel,
    styles: &Styles,
    project: &Path,
    component_id: &str,
) -> Option<ScaffoldManifest> {
    let comp = model.nodes.iter().find(|n| n.id == component_id && n.kind == Kind::Component)?;
    let place = placement(model, styles, project, component_id)?;
    let anchored = |id: &str| model.source_map.get(id).is_some_and(|v| !v.is_empty());
    let symbols = model
        .nodes
        .iter()
        .filter(|n| n.kind == Kind::Symbol && n.parent_id.as_deref() == Some(component_id))
        .map(|n| ScaffoldSymbol {
            node_id: n.id.clone(),
            name: n.name.clone(),
            kind: if n.properties.is_empty() { "behavior".into() } else { "type".into() },
            responsibilities: n.responsibilities.iter().map(|r| strip_markup(&r.statement)).collect(),
            properties: n
                .properties
                .iter()
                .map(|p| if p.description.is_empty() { p.label.clone() } else { format!("{}: {}", p.label, p.description) })
                .collect(),
            anchored: anchored(&n.id) || n.responsibilities.iter().any(|r| anchored(&r.id)),
        })
        .collect();
    let mut existing: Vec<String> = Vec::new();
    let mut push_files = |id: &str| {
        if let Some(locs) = model.source_map.get(id) {
            for l in locs {
                if !existing.contains(&l.pattern) {
                    existing.push(l.pattern.clone());
                }
            }
        }
    };
    for r in &comp.responsibilities {
        push_files(&r.id);
    }
    for n in model.nodes.iter().filter(|n| n.parent_id.as_deref() == Some(component_id)) {
        push_files(&n.id);
        for r in &n.responsibilities {
            push_files(&r.id);
        }
    }
    Some(ScaffoldManifest {
        node_id: comp.id.clone(),
        name: comp.name.clone(),
        style: place.style,
        layer: place.layer,
        dir: place.dir,
        basename: snake_case(&comp.name),
        may_import: place.may_import,
        responsibilities: comp.responsibilities.iter().map(|r| strip_markup(&r.statement)).collect(),
        symbols,
        existing_files: existing,
    })
}

/// Is `layer` a legal tag for a component under `parent_id`? The answer names
/// what to fix: an unstyled container, an unknown style, or a layer outside
/// the style's list. Shared by every write path that sets a layer.
pub fn check_layer(
    model: &ScryModel,
    styles: &Styles,
    parent_id: &str,
    layer: &str,
) -> Result<(), String> {
    let parent_name = model
        .nodes
        .iter()
        .find(|n| n.id == parent_id)
        .map(|n| n.name.as_str())
        .unwrap_or(parent_id);
    let Some(style_name) = governing_style(model, parent_id) else {
        return Err(format!(
            "Container '{parent_name}' has no style, so no layer can be checked — set one first \
             (update_nodes {{style}}; known styles: {})",
            styles.names().join(", ")
        ));
    };
    let Some(def) = styles.get(style_name) else {
        return Err(format!(
            "Container '{parent_name}' has unknown style '{style_name}' — known styles: {}",
            styles.names().join(", ")
        ));
    };
    let layer = layer.trim();
    if layer.is_empty() {
        return Err(format!(
            "Components under '{parent_name}' ({style_name}) need a layer — one of: {}",
            def.layer_names().join(", ")
        ));
    }
    if !def.has_layer(layer) {
        return Err(format!(
            "Layer '{layer}' is not in style '{style_name}' — one of: {}",
            def.layer_names().join(", ")
        ));
    }
    Ok(())
}

// --- Built-ins ----------------------------------------------------------------

fn layers(defs: &[(&str, &str)]) -> Vec<LayerDef> {
    defs.iter()
        .map(|(n, d)| LayerDef { name: (*n).into(), description: (*d).into() })
        .collect()
}

fn matrix(rows: &[(&str, &[&str])]) -> BTreeMap<String, Vec<String>> {
    rows.iter()
        .map(|(from, to)| ((*from).into(), to.iter().map(|s| (*s).to_string()).collect()))
        .collect()
}

fn strs(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| (*s).to_string()).collect()
}

fn dirs(rows: &[(&str, &[&str])]) -> BTreeMap<String, Vec<String>> {
    matrix(rows)
}

/// Packages that do I/O or bind to a framework — nothing a pure core may touch.
const IO_PACKAGES: &[&str] = &[
    "react", "react-dom", "vue", "svelte", "@angular/core", "next", "astro",
    "express", "fastify", "koa", "hono", "@nestjs/core",
    "axum", "actix-web", "rocket", "tokio", "sqlx", "diesel", "reqwest", "rusqlite",
    "prisma", "@prisma/client", "typeorm", "sequelize", "mongoose", "pg", "mysql2",
    "django", "flask", "fastapi", "sqlalchemy", "requests", "httpx",
    "net/http", "database/sql", "gorm.io/gorm",
];

/// Cockburn's hexagon with the four layers most codebases use inside it.
pub fn hexagonal() -> StyleDef {
    StyleDef {
        name: "hexagonal".into(),
        description: "services, backends, library cores".into(),
        layers: layers(&[
            ("presentation", "entry points that drive the application: handlers, controllers, CLI commands, UI"),
            ("infrastructure", "adapters the application drives: storage, network, filesystem, third parties"),
            ("application", "use cases and the ports they expose; orchestrates the domain"),
            ("domain", "the business model and its rules; pure, no I/O, no framework"),
        ]),
        matrix: matrix(&[
            ("domain", &["domain"]),
            ("application", &["application", "domain"]),
            ("infrastructure", &["infrastructure", "application", "domain"]),
            ("presentation", &["presentation", "application"]),
        ]),
        isolation: Isolation::Inclusive,
        inbound: strs(&["presentation", "application"]),
        outbound: strs(&["infrastructure"]),
        public_surface: strs(&["index.ts", "index.js", "mod.rs", "lib.rs", "__init__.py"]),
        external_bans: [("domain".to_string(), strs(IO_PACKAGES))].into_iter().collect(),
        path: PathConvention {
            dirs: dirs(&[
                ("presentation", &["presentation", "controllers", "handlers", "api", "cli", "ui"]),
                ("infrastructure", &["infrastructure", "infra", "adapters"]),
                ("application", &["application", "usecases", "use-cases", "use_cases"]),
                ("domain", &["domain"]),
            ]),
            markers: dirs(&[
                ("presentation", &[".controller.", ".handler.", ".route."]),
                ("infrastructure", &[".adapter.", ".repository.", ".client."]),
                ("application", &[".use-case.", ".usecase.", ".port.", ".service."]),
                ("domain", &[".entity.", ".value.", ".aggregate."]),
            ]),
        },
        drawing: Drawing::Hexagon,
    }
}

/// Feature-Sliced Design 2.1: six layers, strictly downward imports, slices on
/// a layer isolated from each other.
pub fn feature_sliced() -> StyleDef {
    StyleDef {
        name: "feature-sliced".into(),
        description: "SPAs, React/Vue/Svelte apps, docs and static sites".into(),
        layers: layers(&[
            ("app", "everything that makes the app run: routing, providers, global styles, config"),
            ("pages", "full pages or large parts of a page in nested routing"),
            ("widgets", "large self-contained chunks of functionality or UI"),
            ("features", "reused implementations of product features: actions users take"),
            ("entities", "business entities the project works with"),
            ("shared", "reusable functionality detached from the product: UI kit, utils, API client"),
        ]),
        matrix: matrix(&[
            ("app", &["pages", "widgets", "features", "entities", "shared"]),
            ("pages", &["widgets", "features", "entities", "shared"]),
            ("widgets", &["features", "entities", "shared"]),
            ("features", &["entities", "shared"]),
            ("entities", &["shared"]),
            ("shared", &["shared"]),
        ]),
        isolation: Isolation::Strict,
        inbound: strs(&["app", "pages"]),
        outbound: strs(&["shared", "app"]),
        public_surface: strs(&["index.ts", "index.tsx", "index.js", "index.jsx"]),
        external_bans: BTreeMap::new(),
        path: PathConvention {
            dirs: dirs(&[
                ("app", &["app"]),
                ("pages", &["pages"]),
                ("widgets", &["widgets"]),
                ("features", &["features"]),
                ("entities", &["entities"]),
                ("shared", &["shared"]),
            ]),
            markers: BTreeMap::new(),
        },
        drawing: Drawing::Rows,
    }
}

/// Functional core, imperative shell. Rust's `main.rs` / `lib.rs` split is the
/// canonical instance.
pub fn core_shell() -> StyleDef {
    StyleDef {
        name: "core-shell".into(),
        description: "CLIs, small libraries, config and script repos".into(),
        layers: layers(&[
            ("shell", "the imperative edge: I/O, arguments, wiring, side effects"),
            ("core", "pure logic with no I/O; everything the shell calls into"),
        ]),
        matrix: matrix(&[("shell", &["shell", "core"]), ("core", &["core"])]),
        isolation: Isolation::Inclusive,
        inbound: strs(&["shell"]),
        outbound: strs(&["shell"]),
        public_surface: strs(&["index.ts", "index.js", "mod.rs", "lib.rs", "__init__.py"]),
        external_bans: [("core".to_string(), strs(IO_PACKAGES))].into_iter().collect(),
        path: PathConvention {
            dirs: dirs(&[
                ("shell", &["shell", "bin", "cli", "commands"]),
                ("core", &["core"]),
            ]),
            markers: dirs(&[("shell", &["main.rs"]), ("core", &["lib.rs"])]),
        },
        drawing: Drawing::Rings,
    }
}

/// dbt's staging → intermediate → marts, with sources at the front.
pub fn pipeline() -> StyleDef {
    StyleDef {
        name: "pipeline".into(),
        description: "ETL, dbt, data work".into(),
        layers: layers(&[
            ("source", "raw inputs as they arrive; never transformed here"),
            ("staging", "one model per source, renamed and typed, nothing joined"),
            ("intermediate", "joins and reshaping between staging and marts"),
            ("marts", "the facts and dimensions consumers read"),
        ]),
        matrix: matrix(&[
            ("source", &[]),
            ("staging", &["source"]),
            ("intermediate", &["staging", "intermediate"]),
            ("marts", &["intermediate", "staging", "marts"]),
        ]),
        isolation: Isolation::Inclusive,
        inbound: strs(&["marts"]),
        outbound: strs(&["source"]),
        public_surface: Vec::new(),
        external_bans: BTreeMap::new(),
        path: PathConvention {
            dirs: dirs(&[
                ("source", &["sources", "source", "raw"]),
                ("staging", &["staging"]),
                ("intermediate", &["intermediate"]),
                ("marts", &["marts"]),
            ]),
            markers: dirs(&[
                ("staging", &["stg_"]),
                ("intermediate", &["int_"]),
                ("marts", &["fct_", "dim_"]),
            ]),
        },
        drawing: Drawing::Columns,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_matrices_only_name_their_own_layers() {
        for s in Styles::builtin().iter() {
            for (from, tos) in &s.matrix {
                assert!(s.has_layer(from), "{}: matrix row '{from}' is not a layer", s.name);
                for to in tos {
                    assert!(s.has_layer(to), "{}: '{from}' → '{to}' names no layer", s.name);
                }
            }
            for l in s.inbound.iter().chain(&s.outbound) {
                assert!(s.has_layer(l), "{}: inbound/outbound '{l}' is not a layer", s.name);
            }
            for (l, _) in &s.path.dirs {
                assert!(s.has_layer(l), "{}: path dir key '{l}' is not a layer", s.name);
            }
        }
    }

    #[test]
    fn hexagonal_matrix_matches_the_spec() {
        let h = hexagonal();
        assert!(h.may_depend("application", "domain"));
        assert!(h.may_depend("infrastructure", "application"));
        assert!(h.may_depend("presentation", "application"));
        assert!(!h.may_depend("domain", "application"));
        assert!(!h.may_depend("presentation", "infrastructure"));
        assert!(!h.may_depend("infrastructure", "presentation"));
        assert!(!h.may_depend("presentation", "domain"));
    }

    #[test]
    fn feature_sliced_is_strictly_downward() {
        let f = feature_sliced();
        assert!(f.may_depend("pages", "shared"));
        assert!(!f.may_depend("pages", "pages"));
        assert!(!f.may_depend("entities", "features"));
        assert!(f.may_depend("shared", "shared"));
    }

    #[test]
    fn path_convention_reads_innermost_dir_then_marker() {
        let h = hexagonal();
        assert_eq!(h.layer_of_path("src/app/domain/user.rs"), Some("domain"));
        assert_eq!(h.layer_of_path("src/create-user.use-case.ts"), Some("application"));
        assert_eq!(h.layer_of_path("src/lib.rs"), None);
        let c = core_shell();
        assert_eq!(c.layer_of_path("src/main.rs"), Some("shell"));
        assert_eq!(c.layer_of_path("src/lib.rs"), Some("core"));
    }

    #[test]
    fn placement_and_scaffold_read_the_style_and_the_boundary() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("svc/src")).unwrap();
        let mut m = ScryModel::default();
        let mut c = node("svc", Kind::Container, Some("s"));
        c.style = Some("hexagonal".into());
        let mut comp = node("k", Kind::Component, Some("svc"));
        comp.layer = Some("application".into());
        comp.name = "Checkout Flow".into();
        comp.responsibilities = vec![serde_json::from_value(
            serde_json::json!({ "id": "r1", "statement": "**When** paid, **record** it" }),
        )
        .unwrap()];
        let mut sym = node("y", Kind::Symbol, Some("k"));
        sym.name = "Order".into();
        sym.properties = vec![serde_json::from_value(serde_json::json!({ "label": "id", "description": "order id" })).unwrap()];
        m.nodes = vec![node("s", Kind::System, None), c, comp, sym];
        m.boundaries.insert("svc".into(), vec![crate::Source { pattern: "svc/**/*".into(), comment: None }]);

        let styles = Styles::builtin();
        let p = placement(&m, &styles, dir.path(), "y").unwrap();
        assert_eq!(p.layer, "application");
        assert_eq!(p.may_import, vec!["application", "domain"]);
        assert_eq!(p.dir.as_deref(), Some("svc/src/application/"), "src/ exists, so it is used");
        assert_eq!(p.line(), "layer: application · may import: application, domain · path: svc/src/application/");
        assert!(placement(&m, &styles, dir.path(), "svc").is_none());

        let sc = scaffold_manifest(&m, &styles, dir.path(), "k").unwrap();
        assert_eq!(sc.basename, "checkout_flow");
        assert_eq!(sc.responsibilities, vec!["When paid, record it"]);
        assert_eq!(sc.symbols.len(), 1);
        assert_eq!(sc.symbols[0].kind, "type");
        assert_eq!(sc.symbols[0].properties, vec!["id: order id"]);
        assert!(!sc.symbols[0].anchored);
        assert!(scaffold_manifest(&m, &styles, dir.path(), "svc").is_none());
    }

    #[test]
    fn styles_round_trip_through_json() {
        for s in Styles::builtin().iter() {
            let json = serde_json::to_string(s).unwrap();
            let back: StyleDef = serde_json::from_str(&json).unwrap();
            assert_eq!(&back, s);
        }
    }

    #[test]
    fn project_styles_override_builtins_by_name() {
        let dir = tempfile::tempdir().unwrap();
        let styles_dir = dir.path().join(".scryer").join("styles");
        std::fs::create_dir_all(&styles_dir).unwrap();
        let mut custom = core_shell();
        custom.name = "two-tier".into();
        std::fs::write(
            styles_dir.join("two-tier.json"),
            serde_json::to_string(&custom).unwrap(),
        )
        .unwrap();
        std::fs::write(styles_dir.join("broken.json"), "{ not json").unwrap();
        let styles = Styles::load(dir.path());
        assert!(styles.get("two-tier").is_some());
        assert!(styles.get("hexagonal").is_some());
        assert_eq!(styles.names().len(), 5);
    }

    fn node(id: &str, kind: Kind, parent: Option<&str>) -> Node {
        let mut n: Node = serde_json::from_str(&format!(
            r#"{{"id":"{id}","kind":"{}","name":"{id}"}}"#,
            serde_json::to_value(kind).unwrap().as_str().unwrap()
        ))
        .unwrap();
        n.parent_id = parent.map(Into::into);
        n
    }

    #[test]
    fn governing_style_and_layer_resolve_through_the_tree() {
        let mut m = ScryModel::default();
        let mut c = node("c", Kind::Container, Some("s"));
        c.style = Some("hexagonal".into());
        let mut comp = node("k", Kind::Component, Some("c"));
        comp.layer = Some("domain".into());
        let mut comp2 = node("k2", Kind::Component, Some("c"));
        comp2.style = Some("core-shell".into());
        comp2.layer = Some("core".into());
        let sym = node("y", Kind::Symbol, Some("k"));
        m.nodes = vec![node("s", Kind::System, None), c, comp, comp2, sym];

        assert_eq!(governing_style(&m, "c"), Some("hexagonal"));
        assert_eq!(governing_style(&m, "k"), Some("hexagonal"));
        assert_eq!(governing_style(&m, "y"), Some("hexagonal"));
        assert_eq!(governing_style(&m, "k2"), Some("core-shell"), "component overrides container");
        assert_eq!(governing_style(&m, "s"), None);
        assert_eq!(layer_of(&m, "k"), Some("domain"));
        assert_eq!(layer_of(&m, "y"), Some("domain"), "symbol inherits");
        assert_eq!(layer_of(&m, "c"), None);
        assert_eq!(container_of(&m, "y").map(|n| n.id.as_str()), Some("c"));
    }
}
