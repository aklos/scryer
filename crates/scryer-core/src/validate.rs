use crate::style::{self, Styles};
use crate::{Kind, LinkKind, ScryModel};
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// Maximum description length, in Unicode scalar values (characters, not bytes).
/// The frontend hard-caps description editors at the same value, so a UI-authored
/// description never trips this warning.
pub const DESCRIPTION_MAX_CHARS: usize = 200;

/// Maximum technology length, in Unicode scalar values. Technology is a badge
/// ("Next.js 14", "Tauri 2 + React"), rendered as a one-to-two-line tag on the
/// diagram card — agents that evict mechanism prose from responsibilities tend
/// to dump it here, where it belongs in the description instead. The frontend
/// caps its technology editor at the same value.
pub const TECHNOLOGY_MAX_CHARS: usize = 80;

/// Field-shape warnings for a single node (length caps on card-rendered
/// fields). Split out so MCP write tools can accept-and-warn on exactly the
/// nodes they touched, without dragging in unrelated model-wide warnings.
pub fn node_field_warnings(n: &crate::Node) -> Vec<String> {
    let mut warnings = Vec::new();
    if let Some(desc) = &n.description {
        let chars = desc.chars().count();
        if chars > DESCRIPTION_MAX_CHARS {
            warnings.push(format!(
                "Node {} (\"{}\") description exceeds {} characters ({})",
                n.id, n.name, DESCRIPTION_MAX_CHARS, chars
            ));
        }
    }
    if let Some(tech) = &n.technology {
        let chars = tech.chars().count();
        if chars > TECHNOLOGY_MAX_CHARS {
            warnings.push(format!(
                "Node {} (\"{}\") technology exceeds {} characters ({}) — technology is a \
                 short badge (\"Next.js 14\", \"PostgreSQL 16\"); move explanatory prose \
                 into the description",
                n.id, n.name, TECHNOLOGY_MAX_CHARS, chars
            ));
        }
    }
    warnings
}

/// Run structural validation against the built-in styles only. Returns a
/// list of human-readable warnings. Callers with a project path should prefer
/// [`validate_with`] so custom styles under `.scryer/styles/` are honoured.
pub fn validate(model: &ScryModel) -> Vec<String> {
    validate_with(model, &Styles::builtin())
}

/// Run structural validation with the given style table.
pub fn validate_with(model: &ScryModel, styles: &Styles) -> Vec<String> {
    let mut warnings: Vec<String> = Vec::new();

    let node_ids: HashSet<&str> = model.nodes.iter().map(|n| n.id.as_str()).collect();
    let group_ids: HashSet<&str> = model.groups.iter().map(|g| g.id.as_str()).collect();

    // --- Nodes ---
    let mut seen_node_ids: HashSet<&str> = HashSet::new();
    for n in &model.nodes {
        if !seen_node_ids.insert(n.id.as_str()) {
            warnings.push(format!("Duplicate node id: {}", n.id));
        }

        match &n.parent_id {
            Some(pid) => {
                if !node_ids.contains(pid.as_str()) {
                    warnings.push(format!(
                        "Node {} (\"{}\") has parent_id '{}' that doesn't exist",
                        n.id, n.name, pid
                    ));
                } else if let Some(parent) = model.nodes.iter().find(|p| p.id == *pid) {
                    let valid = matches!(
                        (parent.kind, n.kind),
                        (Kind::System, Kind::Container)
                            | (Kind::Container, Kind::Component)
                            | (Kind::Component, Kind::Symbol)
                    );
                    if !valid {
                        warnings.push(format!(
                            "Node {} (\"{}\") kind {:?} cannot have parent of kind {:?}",
                            n.id, n.name, n.kind, parent.kind
                        ));
                    }
                    if parent.external == Some(true) {
                        warnings.push(format!(
                            "Node {} (\"{}\") is a child of external node {} — externals have no children",
                            n.id, n.name, parent.id
                        ));
                    }
                }
            }
            None => {
                if !matches!(n.kind, Kind::Person | Kind::System) {
                    warnings.push(format!(
                        "Node {} (\"{}\") of kind {:?} has no parent — only person/system are top-level",
                        n.id, n.name, n.kind
                    ));
                }
            }
        }

        if n.external == Some(true) && !matches!(n.kind, Kind::System | Kind::Container) {
            warnings.push(format!(
                "Node {} (\"{}\") has external=true but kind is {:?} — only system/container can be external",
                n.id, n.name, n.kind
            ));
        }

        let mut resp_ids: HashSet<&str> = HashSet::new();
        for r in &n.responsibilities {
            if !resp_ids.insert(r.id.as_str()) {
                warnings.push(format!(
                    "Duplicate responsibility id '{}' on node {}",
                    r.id, n.id
                ));
            }
            if r.statement.trim().is_empty() {
                warnings.push(format!(
                    "Empty responsibility statement on node {} (id={})",
                    n.id, r.id
                ));
            }
        }

        warnings.extend(node_field_warnings(n));

        // Symbol names are source identifiers. Types, classes, interfaces, and
        // React components legitimately start uppercase, so an uppercase start
        // is allowed — we only reject names that aren't identifier-shaped at all
        // (spaces, punctuation, leading digits).
        if n.kind == Kind::Symbol && !is_valid_identifier(&n.name, true) {
            warnings.push(format!(
                "Symbol node {} (\"{}\") name should be a valid identifier",
                n.id, n.name
            ));
        }

        // Empty symbol — carries no semantic content of its own: no
        // responsibility, no declared data shape. Such a
        // node justifies nothing on the diagram. This is a flag, not a hard
        // error: the agent must resolve each one by giving it a business
        // responsibility or removing it (folding it into the parent symbol that
        // uses it). Scoped to symbols — structural nodes carry meaning through
        // their children.
        if crate::is_node_empty(n) {
            warnings.push(format!(
                "Symbol node {} (\"{}\") is empty — give it a responsibility or data shape, or fold it into its parent and remove it (a link alone does not justify it)",
                n.id, n.name
            ));
        }
    }

    // --- Links ---
    let mut seen_link_ids: HashSet<&str> = HashSet::new();
    for l in &model.links {
        if !seen_link_ids.insert(l.id.as_str()) {
            warnings.push(format!("Duplicate link id: {}", l.id));
        }
        if !node_ids.contains(l.src.as_str()) {
            warnings.push(format!("Link {} has unknown src '{}'", l.id, l.src));
        }
        if !node_ids.contains(l.dst.as_str()) {
            warnings.push(format!("Link {} has unknown dst '{}'", l.id, l.dst));
        }
        if l.src == l.dst {
            warnings.push(format!(
                "Link {} has src == dst ({}) — self-links are invalid",
                l.id, l.src
            ));
        } else if let Some(v) = link_violation(model, &l.src, &l.dst) {
            warnings.push(describe_violation(model, &l.src, &l.dst, &v));
        }
    }

    // --- Groups ---
    let mut seen_group_ids: HashSet<&str> = HashSet::new();
    for g in &model.groups {
        if !seen_group_ids.insert(g.id.as_str()) {
            warnings.push(format!("Duplicate group id: {}", g.id));
        }
        if let Some(pgid) = &g.parent_group_id {
            if !group_ids.contains(pgid.as_str()) {
                warnings.push(format!(
                    "Group {} has parent_group_id '{}' that doesn't exist",
                    g.id, pgid
                ));
            }
        }
        if g.member_ids.is_empty() {
            warnings.push(format!(
                "Group {} (\"{}\") has no members — set memberIds to the node ids it groups, or delete it",
                g.id, g.name
            ));
        }
        match &g.parent_node_id {
            None => {
                if g.parent_group_id.is_none() {
                    warnings.push(format!(
                        "Group {} (\"{}\") has no parentNodeId — it renders at the top level instead of inside its parent node's diagram; set parentNodeId to the node whose children it groups",
                        g.id, g.name
                    ));
                }
            }
            Some(pnid) => match model.nodes.iter().find(|n| n.id == *pnid) {
                None => warnings.push(format!(
                    "Group {} has parentNodeId '{}' that doesn't exist",
                    g.id, pnid
                )),
                Some(_) => {
                    for mid in &g.member_ids {
                        if let Some(member) = model.nodes.iter().find(|n| n.id == *mid) {
                            if member.parent_id.as_deref() != Some(pnid.as_str()) {
                                warnings.push(format!(
                                    "Group {} member '{}' is not a child of parentNodeId '{}' — group members must be children of the node the group is anchored to",
                                    g.id, mid, pnid
                                ));
                            }
                        }
                    }
                }
            },
        }
        if let Some(desc) = &g.description {
            let chars = desc.chars().count();
            if chars > DESCRIPTION_MAX_CHARS {
                warnings.push(format!(
                    "Group {} (\"{}\") description exceeds {} characters ({})",
                    g.id, g.name, DESCRIPTION_MAX_CHARS, chars
                ));
            }
        }
        let mut member_kinds: HashSet<&str> = HashSet::new();
        for mid in &g.member_ids {
            match model.nodes.iter().find(|n| n.id == *mid) {
                Some(member) => {
                    member_kinds.insert(kind_name(&member.kind));
                }
                None => warnings.push(format!(
                    "Group {} member '{}' is not a node",
                    g.id, mid
                )),
            }
        }
        if member_kinds.len() > 1 {
            warnings.push(format!(
                "Group {} mixes member kinds ({:?}) — all members must be at the same level",
                g.id, member_kinds
            ));
        }

        let mut resp_ids: HashSet<&str> = HashSet::new();
        for r in &g.responsibilities {
            if !resp_ids.insert(r.id.as_str()) {
                warnings.push(format!(
                    "Duplicate responsibility id '{}' on group {}",
                    r.id, g.id
                ));
            }
            if r.statement.trim().is_empty() {
                warnings.push(format!(
                    "Empty responsibility statement on group {} (id={})",
                    g.id, r.id
                ));
            }
        }
    }

    // --- Responsibility id global uniqueness ---
    // Responsibility ids must be unique across the WHOLE model (every node AND
    // group), not merely within a host: `find_responsibility`, the id minters
    // (`next_*_id_union`, `IdMinter::absorb`), and every source_map / fold lookup
    // key by id alone and assume a single home. The per-host checks above catch a
    // repeat on the same host; this catches the same id living on two hosts,
    // where a lookup silently binds to whichever the model lists first.
    let mut resp_hosts: HashMap<&str, Vec<&str>> = HashMap::new();
    for n in &model.nodes {
        for r in &n.responsibilities {
            resp_hosts.entry(r.id.as_str()).or_default().push(n.id.as_str());
        }
    }
    for g in &model.groups {
        for r in &g.responsibilities {
            resp_hosts.entry(r.id.as_str()).or_default().push(g.id.as_str());
        }
    }
    let mut collisions: Vec<String> = Vec::new();
    for (rid, hosts) in &resp_hosts {
        // Distinct hosts only — a repeat on one host is already reported above.
        let mut distinct: Vec<&str> = hosts.iter().copied().collect::<HashSet<&str>>().into_iter().collect();
        if distinct.len() > 1 {
            distinct.sort_unstable();
            collisions.push(format!(
                "Responsibility id '{}' is used on multiple hosts ({}) — responsibility ids must be globally unique",
                rid,
                distinct.join(", ")
            ));
        }
    }
    // HashMap order is nondeterministic; sort so the warning list is stable.
    collisions.sort();
    warnings.extend(collisions);

    // --- Code-side mapping keys ---
    // source_map is keyed by responsibility id, or by a schema node id (a
    // schema's declaration site); boundaries by node id.
    let resp_ids: HashSet<&str> = model
        .nodes
        .iter()
        .flat_map(|n| n.responsibilities.iter())
        .chain(model.groups.iter().flat_map(|g| g.responsibilities.iter()))
        .map(|r| r.id.as_str())
        .collect();
    // Symbols that declare a data shape map their declaration location by node id.
    let property_node_ids: HashSet<&str> = model
        .nodes
        .iter()
        .filter(|n| !n.properties.is_empty())
        .map(|n| n.id.as_str())
        .collect();
    for id in model.source_map.keys() {
        if !resp_ids.contains(id.as_str()) && !property_node_ids.contains(id.as_str()) {
            warnings.push(format!(
                "Source map references unknown responsibility or property-bearing node '{}'",
                id
            ));
        }
    }
    // test_map is keyed by responsibility id only: tests attach to claims.
    for id in model.test_map.keys() {
        if !resp_ids.contains(id.as_str()) {
            warnings.push(format!(
                "Test map references unknown responsibility '{}' — tests attach to live \
                 claims; fix the id or clear the entry (update_source_map test_entries \
                 with empty locations)",
                id
            ));
        }
    }
    for id in model.boundaries.keys() {
        if !node_ids.contains(id.as_str()) {
            warnings.push(format!("Boundary references unknown node '{}'", id));
        }
    }
    // A boundary glob with no directory prefix (`**/*`, `*.rs`) owns every
    // otherwise-unowned file in the repository — one such glob silently poisons
    // drift and coverage attribution for the whole project.
    for (id, sources) in &model.boundaries {
        for s in sources {
            if crate::ownership::pattern_specificity(&s.pattern) == 0 {
                warnings.push(format!(
                    "Boundary glob '{}' on '{}' has no directory prefix — it owns every \
                     otherwise-unowned file in the repository, so unrelated changes anywhere \
                     attribute to this node in drift and coverage. Scope it to the node's real \
                     code region (e.g. 'src/**/*')",
                    s.pattern,
                    name_of(model, id),
                ));
            }
        }
    }

    warnings.extend(check_disconnected(model));
    warnings.extend(check_styles(model, styles));

    // The same fact must never cost two lines: every duplicated warning teaches
    // the reader to skim, and a skimmed gate is no gate.
    let mut seen: HashSet<String> = HashSet::new();
    warnings.retain(|w| seen.insert(w.clone()));

    warnings
}

/// The subset of model problems that are *structural invariant violations*: an
/// id that downstream code treats as unique actually names two things, so an
/// id-keyed lookup silently binds to whichever host the model lists first
/// (`find_responsibility`, `commit.rs`) and the plan diff's id-keyed index
/// (`diff::index_responsibilities` / `indexResponsibilities`) keeps only one
/// copy per id, dropping the other without a trace.
///
/// Unlike the advisory findings [`validate`] also returns (length caps,
/// disconnected nodes, link legality — all legitimate transient states), these
/// are never a valid *committed* state. A duplicate that reaches `model.scry` is
/// invisible to the plan diff: `from` and `to` both collapse to a single copy,
/// they compare equal, no change is emitted, and the stale wrong copy sits
/// committed indefinitely. [`crate::write_model_at`] gates every committed write
/// on this returning empty, so the committed layer can never hold one.
///
/// Returns stable, de-duplicated messages (empty ⇒ no violation).
pub fn structural_violations(model: &ScryModel) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();

    // Duplicate node / link / group ids — each is an identity every by-id lookup
    // and every diff index assumes is unique.
    let mut seen_nodes: HashSet<&str> = HashSet::new();
    for n in &model.nodes {
        if !seen_nodes.insert(n.id.as_str()) {
            out.push(format!("Duplicate node id: {}", n.id));
        }
    }
    let mut seen_links: HashSet<&str> = HashSet::new();
    for l in &model.links {
        if !seen_links.insert(l.id.as_str()) {
            out.push(format!("Duplicate link id: {}", l.id));
        }
    }
    let mut seen_groups: HashSet<&str> = HashSet::new();
    for g in &model.groups {
        if !seen_groups.insert(g.id.as_str()) {
            out.push(format!("Duplicate group id: {}", g.id));
        }
    }

    // A property is keyed by (owner node, label); a repeated label on one node
    // collapses in the diff's property index just as a responsibility id does.
    for n in &model.nodes {
        let mut seen: HashSet<&str> = HashSet::new();
        for p in &n.properties {
            if !seen.insert(p.label.as_str()) {
                out.push(format!(
                    "Duplicate property label '{}' on node {}",
                    p.label, n.id
                ));
            }
        }
    }

    // Responsibility ids must be globally unique across every node AND group.
    // Both a repeat on one host and the same id living on two hosts are
    // silent-misbind states, so report each. (This mirrors the invariant
    // [`validate`] reports as an advisory; here it is the gating subset.)
    let mut resp_hosts: HashMap<&str, Vec<&str>> = HashMap::new();
    for n in &model.nodes {
        let mut per_host: HashSet<&str> = HashSet::new();
        for r in &n.responsibilities {
            if !per_host.insert(r.id.as_str()) {
                out.push(format!("Duplicate responsibility id '{}' on node {}", r.id, n.id));
            }
            resp_hosts.entry(r.id.as_str()).or_default().push(n.id.as_str());
        }
    }
    for g in &model.groups {
        let mut per_host: HashSet<&str> = HashSet::new();
        for r in &g.responsibilities {
            if !per_host.insert(r.id.as_str()) {
                out.push(format!("Duplicate responsibility id '{}' on group {}", r.id, g.id));
            }
            resp_hosts.entry(r.id.as_str()).or_default().push(g.id.as_str());
        }
    }
    for (rid, hosts) in &resp_hosts {
        let mut distinct: Vec<&str> =
            hosts.iter().copied().collect::<HashSet<&str>>().into_iter().collect();
        if distinct.len() > 1 {
            distinct.sort_unstable();
            out.push(format!(
                "Responsibility id '{}' is used on multiple hosts ({}) — responsibility ids must be globally unique",
                rid,
                distinct.join(", ")
            ));
        }
    }

    // HashMap iteration is nondeterministic; sort + de-dup for a stable message.
    out.sort();
    let mut seen: HashSet<String> = HashSet::new();
    out.retain(|w| seen.insert(w.clone()));
    out
}

/// The horizontal axis: every non-external container declares a style, every
/// component under a styled container carries one of that style's layers, and
/// every link between styled nodes says what kind of link it is. Membership is
/// checked here so a typo never reaches the matrix; legality of the pairs is
/// [`link_violation`]'s job.
pub fn check_styles(model: &ScryModel, styles: &Styles) -> Vec<String> {
    let mut warnings: Vec<String> = Vec::new();
    let style_names = styles.names().join(", ");

    for n in &model.nodes {
        match n.kind {
            Kind::Container => {
                if n.external == Some(true) {
                    continue;
                }
                match n.style.as_deref() {
                    None => warnings.push(format!(
                        "Container {} (\"{}\") has no style — every container declares one \
                         ({style_names}); set it with update_nodes {{style}}",
                        n.id, n.name
                    )),
                    Some(s) if styles.get(s).is_none() => warnings.push(format!(
                        "Container {} (\"{}\") has unknown style '{}' — known styles: {style_names}",
                        n.id, n.name, s
                    )),
                    _ => {}
                }
                if n.layer.is_some() {
                    warnings.push(format!(
                        "Container {} (\"{}\") carries a layer — layers belong on components only",
                        n.id, n.name
                    ));
                }
            }
            Kind::Component => {
                if let Some(s) = n.style.as_deref() {
                    if styles.get(s).is_none() {
                        warnings.push(format!(
                            "Component {} (\"{}\") has unknown style '{}' — known styles: {style_names}",
                            n.id, n.name, s
                        ));
                    }
                }
                // Without a governing style the container warning above already
                // says what to fix; a second line per component adds nothing.
                let Some(def) = style::governing_style(model, &n.id).and_then(|s| styles.get(s)) else {
                    continue;
                };
                let layer_names = def.layer_names().join(", ");
                match n.layer.as_deref() {
                    None => warnings.push(format!(
                        "Component {} (\"{}\") has no layer — its style '{}' needs one of: {layer_names}; \
                         set it with update_nodes {{layer}}",
                        n.id, n.name, def.name
                    )),
                    Some(l) if !def.has_layer(l) => warnings.push(format!(
                        "Component {} (\"{}\") has layer '{}', which is not in style '{}' ({layer_names})",
                        n.id, n.name, l, def.name
                    )),
                    _ => {}
                }
            }
            Kind::Person | Kind::System | Kind::Symbol => {
                if n.style.is_some() {
                    warnings.push(format!(
                        "{} {} (\"{}\") carries a style — styles belong on containers (or a component overriding its container)",
                        kind_name(&n.kind), n.id, n.name
                    ));
                }
                if n.layer.is_some() {
                    warnings.push(format!(
                        "{} {} (\"{}\") carries a layer — layers belong on components only; symbols inherit theirs",
                        kind_name(&n.kind), n.id, n.name
                    ));
                }
            }
        }
    }

    // Links between two styled nodes (component or symbol level under a
    // container that declares a style) carry a kind. Prose-only links stay
    // legal at system and container level.
    let styled = |id: &str| {
        matches!(
            model.nodes.iter().find(|n| n.id == id).map(|n| n.kind),
            Some(Kind::Component | Kind::Symbol)
        ) && style::governing_style(model, id).is_some()
    };
    for l in &model.links {
        if l.kind.is_none() && styled(&l.src) && styled(&l.dst) {
            warnings.push(format!(
                "Link {} ({} → {}) has no kind — links inside a styled container say what they are: \
                 implements | calls | uses | depends (update_links {{kind}})",
                l.id,
                name_of(model, &l.src),
                name_of(model, &l.dst)
            ));
        }
        if let Some(v) = style_link_violation(model, styles, &l.src, &l.dst, l.kind) {
            warnings.push(format!("Link {}: {v}", l.id));
        }
    }

    warnings.extend(check_unreached(model, styles));
    warnings.extend(check_file_listing(model));
    warnings
}

/// Check a (prospective or existing) link against its style's legality
/// matrix. `None` when legal or when the style says nothing about the pair.
///
/// Inside one container the `(layer(src), layer(dst))` pair must be in the
/// matrix, and a same-layer link between two different components must be
/// `kind: uses` (a sibling reached through its public surface). A link that
/// enters a styled container from outside must land on one of its inbound
/// layers (hex: presentation or application; FSD: app or pages; core-shell:
/// shell), never deeper. Symbols carry their component's layer.
pub fn style_link_violation(
    model: &ScryModel,
    styles: &Styles,
    src: &str,
    dst: &str,
    kind: Option<LinkKind>,
) -> Option<String> {
    let src_c = style::container_of(model, src)?;
    let dst_c = style::container_of(model, dst)?;
    let dst_layer = style::layer_of(model, dst)?;
    let def = styles.get(style::governing_style(model, dst)?)?;
    let src_name = name_of(model, src);
    let dst_name = name_of(model, dst);

    if src_c.id != dst_c.id {
        // Entering from outside: land on an inbound layer.
        if def.is_inbound(dst_layer) || def.inbound.is_empty() {
            return None;
        }
        return Some(format!(
            "'{src_name}' enters container '{}' ({}) at '{dst_name}', which is on layer \
             '{dst_layer}' — links from outside land on {}; link to a node on that layer \
             (or to the container itself) instead",
            dst_c.name,
            def.name,
            def.inbound.join(" or ")
        ));
    }

    let src_layer = style::layer_of(model, src)?;
    // The importer's style governs; a component overriding its container's
    // style is checked by its own table.
    let def = styles.get(style::governing_style(model, src)?)?;
    if !def.may_depend(src_layer, dst_layer) {
        let allowed = def.allowed(src_layer);
        return Some(format!(
            "'{src_name}' ({src_layer}) → '{dst_name}' ({dst_layer}) is illegal in style '{}': \
             {src_layer} may depend on {}",
            def.name,
            if allowed.is_empty() { "nothing".to_string() } else { allowed.join(", ") }
        ));
    }
    let src_comp = style::layer_component(model, src)?;
    let dst_comp = style::layer_component(model, dst)?;
    if src_layer == dst_layer && src_comp.id != dst_comp.id && kind != Some(LinkKind::Uses) {
        return Some(format!(
            "'{src_name}' → '{dst_name}' joins two {src_layer} components — a same-layer link \
             reaches a sibling through its public surface and is `kind: uses`",
        ));
    }
    None
}

/// A component on an inbound layer other than the outermost (hex:
/// application; FSD: pages) that nothing links INTO is a use case nobody
/// drives. Reached means: any link whose dst is the component or one of its
/// symbols, from anywhere — a presentation sibling, another container, a
/// person.
fn check_unreached(model: &ScryModel, styles: &Styles) -> Vec<String> {
    let mut warnings = Vec::new();
    let by_id: HashMap<&str, &crate::Node> = model.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    let reached: HashSet<&str> = model
        .links
        .iter()
        .filter_map(|l| {
            let d = by_id.get(l.dst.as_str())?;
            Some(match d.kind {
                Kind::Symbol => d.parent_id.as_deref().unwrap_or(d.id.as_str()),
                _ => d.id.as_str(),
            })
        })
        .collect();
    for n in &model.nodes {
        if n.kind != Kind::Component {
            continue;
        }
        let (Some(layer), Some(def)) = (
            n.layer.as_deref(),
            style::governing_style(model, &n.id).and_then(|s| styles.get(s)),
        ) else {
            continue;
        };
        let inner_inbound = def.inbound.iter().skip(1).any(|l| l == layer);
        if inner_inbound && !reached.contains(n.id.as_str()) {
            let Some(container) = style::container_of(model, &n.id) else { continue };
            warnings.push(format!(
                "Component {} (\"{}\", {layer}) in '{}' has nothing linking into it — \
                 {layer} components are driven by {} or by a link from outside the container; \
                 add the link or fold the component away",
                n.id, n.name, container.name, def.inbound[0]
            ));
        }
    }
    warnings
}

/// Minimum component count before the file-listing signature is trusted.
const FILE_LISTING_MIN_COMPONENTS: usize = 4;

/// Rule 7's checkable proxy. Cohesion is not computable, but "one component
/// per file" has a signature that is: every anchored component in a container
/// maps to exactly one file, and no file is shared between two of them. That
/// is a directory listing wearing component names, not a decomposition.
fn check_file_listing(model: &ScryModel) -> Vec<String> {
    let mut warnings = Vec::new();
    let by_id: HashMap<&str, &crate::Node> = model.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    // component id → files its claims (own + symbols') anchor into.
    let mut files: HashMap<&str, HashSet<&str>> = HashMap::new();
    for n in &model.nodes {
        let comp = match n.kind {
            Kind::Component => n.id.as_str(),
            Kind::Symbol => match n.parent_id.as_deref().and_then(|p| by_id.get(p)) {
                Some(p) if p.kind == Kind::Component => p.id.as_str(),
                _ => continue,
            },
            _ => continue,
        };
        let entry = files.entry(comp).or_default();
        for r in &n.responsibilities {
            if let Some(locs) = model.source_map.get(&r.id) {
                entry.extend(locs.iter().map(|l| l.pattern.as_str()));
            }
        }
        if let Some(locs) = model.source_map.get(&n.id) {
            entry.extend(locs.iter().map(|l| l.pattern.as_str()));
        }
    }
    for container in model.nodes.iter().filter(|n| n.kind == Kind::Container) {
        let anchored: Vec<(&str, &HashSet<&str>)> = model
            .nodes
            .iter()
            .filter(|n| n.kind == Kind::Component && n.parent_id.as_deref() == Some(container.id.as_str()))
            .filter_map(|n| files.get(n.id.as_str()).filter(|f| !f.is_empty()).map(|f| (n.id.as_str(), f)))
            .collect();
        if anchored.len() < FILE_LISTING_MIN_COMPONENTS {
            continue;
        }
        let one_each = anchored.iter().all(|(_, f)| f.len() == 1);
        let distinct: HashSet<&str> = anchored.iter().flat_map(|(_, f)| f.iter().copied()).collect();
        if one_each && distinct.len() == anchored.len() {
            warnings.push(format!(
                "Container {} (\"{}\") reads as a file listing, not a decomposition: all {} anchored \
                 components map to exactly one file each and no file is shared. Rule 7: cluster \
                 components from cohesion and the dependency graph, several files per component",
                container.id,
                container.name,
                anchored.len()
            ));
        }
    }
    warnings
}

/// Per-level connectivity (the C4 "same level of abstraction" rule). Each C4
/// diagram is one level: the system context shows persons + systems; a system's
/// container view shows its containers plus reference nodes linked into it; a
/// container's component view shows its components plus references; likewise for
/// the code level. A relationship is only meaningful where both endpoints are
/// visible at that level. This flags an owned node that connects to nothing
/// visible at its own level — e.g. an actor that links only to containers,
/// never to the system, so the system context diagram has no relationship for
/// it. A relationship modeled only at a coarser level (parent→X with no child
/// linking to X) is NOT flagged: X simply isn't surfaced on the inner view,
/// which is legal — relationships may be modeled only as deep as they're useful.
fn check_disconnected(model: &ScryModel) -> Vec<String> {
    let mut warnings: Vec<String> = Vec::new();
    let by_id: HashMap<&str, &crate::Node> =
        model.nodes.iter().map(|n| (n.id.as_str(), n)).collect();

    // Does any link connect two nodes both present in `visible`? Mark endpoints.
    let connected_within = |visible: &HashSet<&str>| -> HashSet<&str> {
        let mut connected: HashSet<&str> = HashSet::new();
        for l in &model.links {
            if visible.contains(l.src.as_str()) && visible.contains(l.dst.as_str()) {
                connected.insert(l.src.as_str());
                connected.insert(l.dst.as_str());
            }
        }
        connected
    };
    let has_any_link = |id: &str| model.links.iter().any(|l| l.src == id || l.dst == id);

    let check_level = |owned: &HashSet<&str>,
                       refs: &HashSet<&str>,
                       view: &str,
                       warnings: &mut Vec<String>| {
        let visible: HashSet<&str> = owned.union(refs).copied().collect();
        let connected = connected_within(&visible);
        // Sorted so the warning list is stable across runs (owned is a HashSet).
        let mut ordered: Vec<&str> = owned.iter().copied().collect();
        ordered.sort_unstable_by_key(|oid| (&by_id[oid].name, *oid));
        let mut linkless: Vec<String> = Vec::new();
        for oid in ordered {
            if connected.contains(oid) {
                continue;
            }
            let n = by_id[oid];
            if has_any_link(oid) {
                warnings.push(format!(
                    "'{}' ({}) has links but none at this level — it will appear disconnected in the {}",
                    n.name, kind_name(&n.kind), view
                ));
            } else if owned.len() > 1 && n.kind != Kind::Symbol {
                // Symbols justify themselves through their claims (rule 8) — the
                // dependency graph at code level is legitimately sparse (data
                // types, UI leaves, entry points), so no per-symbol nag here.
                linkless.push(format!("'{}' ({})", n.name, kind_name(&n.kind)));
            }
        }
        // One rolled-up warning per view instead of one per node, so a sparse
        // diagram costs one line, not a wall the agent learns to skip.
        match linkless.len() {
            0 => {}
            1 => warnings.push(format!(
                "{} has no links — it will appear disconnected in the {}",
                linkless[0], view
            )),
            _ => warnings.push(format!(
                "{} have no links — they will appear disconnected in the {}",
                linkless.join(", "),
                view
            )),
        }
    };

    // System context: persons + systems.
    let system_level: HashSet<&str> = model
        .nodes
        .iter()
        .filter(|n| matches!(n.kind, Kind::Person | Kind::System))
        .map(|n| n.id.as_str())
        .collect();
    let empty: HashSet<&str> = HashSet::new();
    check_level(&system_level, &empty, "system context", &mut warnings);

    // For each non-external parent, the view of its direct children (one C4
    // level down): owned = children; refs = the references actually surfaced on
    // that view — outside nodes linked to a direct child (not merely to the
    // parent), matching the canvas projection. A relationship modeled only at
    // the parent level (parent→X with no child link) is not surfaced here and
    // is not flagged: it is a legal coarser-grained relationship.
    for parent in &model.nodes {
        if parent.external == Some(true) {
            continue;
        }
        let child_kind = match parent.kind {
            Kind::System => Kind::Container,
            Kind::Container => Kind::Component,
            Kind::Component => Kind::Symbol, // code level
            _ => continue,
        };
        let owned: HashSet<&str> = model
            .nodes
            .iter()
            .filter(|n| {
                n.parent_id.as_deref() == Some(parent.id.as_str())
                    && n.kind == child_kind
            })
            .map(|n| n.id.as_str())
            .collect();
        if owned.is_empty() {
            continue;
        }
        // Reference nodes: outside the owned set, linked to a direct child
        // (this is what the canvas surfaces on the view), excluding the parent
        // itself and the parent's parent.
        let mut refs: HashSet<&str> = HashSet::new();
        for l in &model.links {
            let touches_child =
                owned.contains(l.src.as_str()) || owned.contains(l.dst.as_str());
            if !touches_child {
                continue;
            }
            for end in [l.src.as_str(), l.dst.as_str()] {
                if end == parent.id || owned.contains(end) {
                    continue;
                }
                if Some(end) == parent.parent_id.as_deref() {
                    continue;
                }
                if by_id.contains_key(end) {
                    refs.insert(end);
                }
            }
        }
        let view = format!("{} view of '{}'", kind_name(&child_kind), parent.name);
        check_level(&owned, &refs, &view, &mut warnings);
    }

    warnings
}

// --- Link legality: the same-level / reference-propagation rule -------------
//
// Relationships connect nodes that share a diagram. Two nodes share a diagram
// when they have the same parent (true siblings). A deeper node may also link
// to a node from outside its surface ONLY when that node is a *reference* on
// the surface — and a reference exists only because the parent (one level up)
// links to it. So a cross-level link is legal iff the parent of the deeper
// endpoint also links to the shallower endpoint, recursively up to a level
// where both are siblings. This is the single source of truth for the rule;
// `add_links` calls it to reject illegal links, and `validate` calls it to
// flag pre-existing ones.

/// Why a link is illegal. Carries node ids; resolve names via `describe_violation`.
pub enum LinkViolation {
    /// One endpoint is an ancestor of the other — containment, not a relationship.
    Containment { ancestor: String, descendant: String },
    /// Same depth, different parents — the two never share a diagram.
    SameLevelDifferentParent,
    /// A deeper→shallower link with no authorizing link from the deeper node's
    /// parent to the shallower endpoint, so the shallower node isn't a reference
    /// on the deeper node's surface.
    UnauthorizedCrossLevel {
        deeper: String,
        other: String,
        parent: String,
    },
}

fn parent_of<'a>(model: &'a ScryModel, id: &str) -> Option<&'a str> {
    model
        .nodes
        .iter()
        .find(|n| n.id == id)
        .and_then(|n| n.parent_id.as_deref())
}

fn depth(model: &ScryModel, id: &str) -> usize {
    let mut d = 0usize;
    let mut cur = id.to_string();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    while let Some(p) = parent_of(model, &cur) {
        if !seen.insert(cur.clone()) {
            break; // cycle guard — a malformed parent chain never loops forever
        }
        d += 1;
        cur = p.to_string();
    }
    d
}

fn is_ancestor(model: &ScryModel, anc: &str, desc: &str) -> bool {
    let mut cur = parent_of(model, desc).map(str::to_string);
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    while let Some(p) = cur {
        if p == anc {
            return true;
        }
        if !seen.insert(p.clone()) {
            break; // cycle guard — a malformed parent chain never loops forever
        }
        cur = parent_of(model, &p).map(str::to_string);
    }
    false
}

fn linked_either(model: &ScryModel, x: &str, y: &str) -> bool {
    model
        .links
        .iter()
        .any(|l| (l.src == x && l.dst == y) || (l.src == y && l.dst == x))
}

fn name_of<'a>(model: &'a ScryModel, id: &'a str) -> &'a str {
    model
        .nodes
        .iter()
        .find(|n| n.id == id)
        .map(|n| n.name.as_str())
        .unwrap_or(id)
}

/// Check a (prospective or existing) link against the same-level rule. Returns
/// `None` when the link is legal. Self-links are caller-handled (returns `None`).
pub fn link_violation(model: &ScryModel, src: &str, dst: &str) -> Option<LinkViolation> {
    if src == dst {
        return None;
    }
    if is_ancestor(model, src, dst) {
        return Some(LinkViolation::Containment {
            ancestor: src.to_string(),
            descendant: dst.to_string(),
        });
    }
    if is_ancestor(model, dst, src) {
        return Some(LinkViolation::Containment {
            ancestor: dst.to_string(),
            descendant: src.to_string(),
        });
    }
    // Same parent (including both top-level) → true siblings, always legal.
    if parent_of(model, src) == parent_of(model, dst) {
        return None;
    }
    let (dsrc, ddst) = (depth(model, src), depth(model, dst));
    if dsrc == ddst {
        return Some(LinkViolation::SameLevelDifferentParent);
    }
    let (deeper, other) = if dsrc > ddst { (src, dst) } else { (dst, src) };
    // `deeper` is strictly deeper than `other`, so it has a parent.
    let parent = match parent_of(model, deeper) {
        Some(p) => p.to_string(),
        None => return None,
    };
    if !linked_either(model, &parent, other) {
        return Some(LinkViolation::UnauthorizedCrossLevel {
            deeper: deeper.to_string(),
            other: other.to_string(),
            parent,
        });
    }
    // The authorizing higher-level link must itself be legal.
    link_violation(model, &parent, other)
}

/// Node ids that `id` may legally link to on its own surface: its siblings plus
/// the references inherited from its parent's links. Used to suggest valid
/// targets in a rejection message.
pub fn link_targets_for(model: &ScryModel, id: &str) -> Vec<String> {
    let parent = parent_of(model, id);
    let mut out: Vec<String> = Vec::new();
    for n in &model.nodes {
        if n.id != id && n.parent_id.as_deref() == parent {
            out.push(n.id.clone());
        }
    }
    if let Some(pp) = parent {
        for l in &model.links {
            let other = if l.src == pp {
                Some(&l.dst)
            } else if l.dst == pp {
                Some(&l.src)
            } else {
                None
            };
            if let Some(o) = other {
                if o.as_str() != id
                    && !is_ancestor(model, o, id)
                    && !is_ancestor(model, id, o)
                    && !out.contains(o)
                {
                    out.push(o.clone());
                }
            }
        }
    }
    out
}

/// Human-readable, corrective explanation of a `LinkViolation`. Shared by
/// `add_links` (rejection) and `validate` (warning).
pub fn describe_violation(
    model: &ScryModel,
    src: &str,
    dst: &str,
    v: &LinkViolation,
) -> String {
    match v {
        LinkViolation::Containment {
            ancestor,
            descendant,
        } => format!(
            "Link {src}→{dst} rejected: '{}' contains '{}' (it is an ancestor in the tree). \
             Containment is expressed by nesting, not by a link — drop this relationship.",
            name_of(model, ancestor),
            name_of(model, descendant)
        ),
        LinkViolation::SameLevelDifferentParent => format!(
            "Link {src}→{dst} rejected: '{}' and '{}' sit at the same level under different \
             parents, so they never share a diagram. Model the relationship between their \
             parents instead — it surfaces as a reference when you drill in.",
            name_of(model, src),
            name_of(model, dst)
        ),
        LinkViolation::UnauthorizedCrossLevel {
            deeper,
            other,
            parent,
        } => {
            let targets = link_targets_for(model, deeper);
            let avail = if targets.is_empty() {
                "(none yet)".to_string()
            } else {
                targets
                    .iter()
                    .map(|t| format!("'{}'", name_of(model, t)))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            format!(
                "Link {src}→{dst} rejected: relationships connect nodes at the same level, and \
                 '{other_name}' is not visible on the surface where '{deeper_name}' lives. First \
                 add a link between '{parent_name}' and '{other_name}' (one level up) — that makes \
                 '{other_name}' a reference on '{deeper_name}'s surface — then '{deeper_name}' may \
                 link to it. Otherwise link '{deeper_name}' to one of the nodes already on its \
                 surface: {avail}.",
                other_name = name_of(model, other),
                deeper_name = name_of(model, deeper),
                parent_name = name_of(model, parent),
            )
        }
    }
}

fn kind_name(k: &Kind) -> &'static str {
    match k {
        Kind::Person => "person",
        Kind::System => "system",
        Kind::Container => "container",
        Kind::Component => "component",
        Kind::Symbol => "symbol",
    }
}

/// Cross-reference the model's source map against the project's filesystem.
/// Catches manifest directories with no source coverage and shared source
/// directories mapped across container boundaries.
pub fn validate_coverage(model: &ScryModel, project_path: &Path) -> Vec<String> {
    let mut warnings: Vec<String> = Vec::new();

    let manifest_dirs = crate::scan::manifest_dirs(project_path);

    // Collect every source pattern from the source map (line-precise) and the
    // boundary globs.
    let mut all_patterns: Vec<&str> = Vec::new();
    for locs in model.source_map.values() {
        for loc in locs {
            all_patterns.push(&loc.pattern);
        }
    }
    for sources in model.boundaries.values() {
        for s in sources {
            all_patterns.push(&s.pattern);
        }
    }

    // Check A: uncovered manifest directories
    for (dir, filename) in &manifest_dirs {
        let prefix = format!("{}/", dir);
        let covered = all_patterns.iter().any(|p| p.starts_with(&prefix) || p == dir);
        if !covered {
            warnings.push(format!(
                "Manifest directory '{}/' (contains {}) is not covered by any source map entry \
                 — this may be a missing compilation unit",
                dir, filename
            ));
        }
    }

    // Check B: cross-container source overlap
    // Build node → container ancestor lookup
    let node_map: HashMap<&str, &crate::Node> =
        model.nodes.iter().map(|n| (n.id.as_str(), n)).collect();

    let mut container_of: HashMap<&str, &str> = HashMap::new();
    for n in &model.nodes {
        let mut cur = n;
        loop {
            if cur.kind == Kind::Container {
                container_of.insert(n.id.as_str(), cur.id.as_str());
                break;
            }
            match &cur.parent_id {
                Some(pid) => match node_map.get(pid.as_str()) {
                    Some(parent) => cur = parent,
                    None => break,
                },
                None => break,
            }
        }
    }

    // Map source directory prefixes to the set of containers they appear under
    let mut dir_to_containers: HashMap<String, HashSet<&str>> = HashMap::new();

    let mut record_pattern = |pattern: &str, node_id: &str| {
        let Some(&container_id) = container_of.get(node_id) else { return };
        // Extract meaningful directory prefix (up to first glob or the parent dir)
        let effective = pattern
            .find(|c: char| c == '*' || c == '?' || c == '{')
            .map(|i| &pattern[..i])
            .unwrap_or(pattern);
        let dir_prefix = if effective.ends_with('/') {
            effective.trim_end_matches('/').to_string()
        } else {
            Path::new(effective)
                .parent()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default()
        };
        if !dir_prefix.is_empty() {
            dir_to_containers
                .entry(dir_prefix)
                .or_default()
                .insert(container_id);
        }
    };

    // source_map is keyed by responsibility id (resolve to its owning node) or
    // by a schema node id directly — either way attribute the pattern to a node.
    let resp_to_node: HashMap<&str, &str> = model
        .nodes
        .iter()
        .flat_map(|n| n.responsibilities.iter().map(move |r| (r.id.as_str(), n.id.as_str())))
        .collect();
    let node_ids_set: HashSet<&str> = model.nodes.iter().map(|n| n.id.as_str()).collect();
    for (key, locs) in &model.source_map {
        let owner = resp_to_node
            .get(key.as_str())
            .copied()
            .or_else(|| node_ids_set.get(key.as_str()).copied());
        if let Some(node_id) = owner {
            for loc in locs {
                record_pattern(&loc.pattern, node_id);
            }
        }
    }
    for (node_id, sources) in &model.boundaries {
        for s in sources {
            record_pattern(&s.pattern, node_id);
        }
    }

    for (dir, containers) in &dir_to_containers {
        if containers.len() > 1 {
            let names: Vec<&str> = {
                let mut v: Vec<&str> = containers
                    .iter()
                    .filter_map(|&id| node_map.get(id).map(|n| n.name.as_str()))
                    .collect();
                v.sort();
                v
            };
            warnings.push(format!(
                "Source directory '{}' is mapped to nodes in multiple containers ({}) \
                 — verify shared library placement",
                dir,
                names.join(", ")
            ));
        }
    }

    warnings
}

fn is_valid_identifier(s: &str, allow_upper_start: bool) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        None => return false,
        Some(c) => {
            if !c.is_ascii_alphabetic() && c != '_' {
                return false;
            }
            if !allow_upper_start && c.is_ascii_uppercase() {
                return false;
            }
        }
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[cfg(test)]
mod style_tests {
    use super::check_styles;
    use crate::style::Styles;
    use crate::{Link, Node, ScryModel};

    fn validate(m: &ScryModel) -> Vec<String> {
        check_styles(m, &Styles::builtin())
    }

    fn node(v: serde_json::Value) -> Node {
        serde_json::from_value(v).unwrap()
    }

    fn model(nodes: Vec<serde_json::Value>) -> ScryModel {
        let mut m = ScryModel::new();
        m.nodes = nodes.into_iter().map(node).collect();
        m
    }

    /// A container with no style, an unknown style, a component with no layer
    /// or a layer outside its style's list — each is one warning naming the fix.
    #[test]
    fn style_and_layer_membership_are_flagged() {
        let m = model(vec![
            serde_json::json!({ "id": "sys", "kind": "system", "name": "S" }),
            serde_json::json!({ "id": "c1", "kind": "container", "name": "Bare", "parentId": "sys" }),
            serde_json::json!({ "id": "c2", "kind": "container", "name": "Odd", "parentId": "sys", "style": "onion" }),
            serde_json::json!({ "id": "c3", "kind": "container", "name": "Hex", "parentId": "sys", "style": "hexagonal" }),
            serde_json::json!({ "id": "k1", "kind": "component", "name": "NoLayer", "parentId": "c3" }),
            serde_json::json!({ "id": "k2", "kind": "component", "name": "BadLayer", "parentId": "c3", "layer": "pages" }),
            serde_json::json!({ "id": "k3", "kind": "component", "name": "Fine", "parentId": "c3", "layer": "domain" }),
            serde_json::json!({ "id": "k4", "kind": "component", "name": "Unchecked", "parentId": "c1" }),
        ]);
        let w = validate(&m);
        let hit = |needle: &str| w.iter().any(|x| x.contains(needle));
        assert!(hit("Container c1 (\"Bare\") has no style"), "{w:?}");
        assert!(hit("Container c2 (\"Odd\") has unknown style 'onion'"), "{w:?}");
        assert!(hit("Component k1 (\"NoLayer\") has no layer"), "{w:?}");
        assert!(hit("Component k2 (\"BadLayer\") has layer 'pages', which is not in style 'hexagonal'"), "{w:?}");
        assert!(!hit("Fine"), "a member layer is silent: {w:?}");
        // Under an unstyled container the container line is the whole story.
        assert!(!hit("Unchecked"), "{w:?}");
    }

    /// External containers own no code and need no style; style on a symbol or
    /// layer on a container is misplaced.
    #[test]
    fn misplaced_tags_and_external_containers() {
        let m = model(vec![
            serde_json::json!({ "id": "sys", "kind": "system", "name": "S" }),
            serde_json::json!({ "id": "ext", "kind": "container", "name": "Stripe", "parentId": "sys", "external": true }),
            serde_json::json!({ "id": "c", "kind": "container", "name": "App", "parentId": "sys", "style": "core-shell", "layer": "core" }),
            serde_json::json!({ "id": "k", "kind": "component", "name": "K", "parentId": "c", "layer": "core" }),
            serde_json::json!({ "id": "y", "kind": "symbol", "name": "y", "parentId": "k", "layer": "core",
                                 "responsibilities": [{ "id": "r1", "statement": "does x" }] }),
        ]);
        let w = validate(&m);
        assert!(!w.iter().any(|x| x.contains("Stripe")), "{w:?}");
        assert!(w.iter().any(|x| x.contains("Container c (\"App\") carries a layer")), "{w:?}");
        assert!(w.iter().any(|x| x.contains("symbol y (\"y\") carries a layer")), "{w:?}");
    }

    fn hex_model() -> ScryModel {
        model(vec![
            serde_json::json!({ "id": "sys", "kind": "system", "name": "S" }),
            serde_json::json!({ "id": "svc", "kind": "container", "name": "Svc", "parentId": "sys", "style": "hexagonal" }),
            serde_json::json!({ "id": "ui", "kind": "container", "name": "Ui", "parentId": "sys", "style": "feature-sliced" }),
            serde_json::json!({ "id": "pres", "kind": "component", "name": "Http", "parentId": "svc", "layer": "presentation" }),
            serde_json::json!({ "id": "app", "kind": "component", "name": "Checkout", "parentId": "svc", "layer": "application" }),
            serde_json::json!({ "id": "app2", "kind": "component", "name": "Refunds", "parentId": "svc", "layer": "application" }),
            serde_json::json!({ "id": "dom", "kind": "component", "name": "Orders", "parentId": "svc", "layer": "domain" }),
            serde_json::json!({ "id": "infra", "kind": "component", "name": "Postgres", "parentId": "svc", "layer": "infrastructure" }),
            serde_json::json!({ "id": "dom_sym", "kind": "symbol", "name": "Order", "parentId": "dom",
                                 "responsibilities": [{ "id": "r1", "statement": "holds lines" }] }),
            serde_json::json!({ "id": "shell", "kind": "component", "name": "Shell", "parentId": "ui", "layer": "app" }),
            serde_json::json!({ "id": "page", "kind": "component", "name": "CheckoutPage", "parentId": "ui", "layer": "pages" }),
        ])
    }

    fn violation(m: &ScryModel, src: &str, dst: &str, kind: Option<crate::LinkKind>) -> Option<String> {
        super::style_link_violation(m, &Styles::builtin(), src, dst, kind)
    }

    /// The matrix decides: downward pairs pass, upward and sibling-edge pairs
    /// are named with the layers the source may depend on.
    #[test]
    fn style_matrix_rejects_illegal_layer_pairs() {
        use crate::LinkKind::*;
        let m = hex_model();
        assert_eq!(violation(&m, "pres", "app", Some(Calls)), None);
        assert_eq!(violation(&m, "app", "dom", Some(Depends)), None);
        assert_eq!(violation(&m, "infra", "app", Some(Implements)), None);
        assert_eq!(violation(&m, "app", "dom_sym", Some(Depends)), None, "symbols carry their component's layer");
        let v = violation(&m, "dom", "app", Some(Calls)).unwrap();
        assert!(v.contains("'Orders' (domain) → 'Checkout' (application) is illegal in style 'hexagonal'"), "{v}");
        assert!(v.contains("domain may depend on domain"), "{v}");
        let v = violation(&m, "pres", "infra", Some(Calls)).unwrap();
        assert!(v.contains("presentation may depend on presentation, application"), "{v}");
        let v = violation(&m, "pres", "dom", Some(Depends)).unwrap();
        assert!(v.contains("is illegal"), "{v}");
    }

    /// Two components on one layer join only through `uses`; within one
    /// component (symbol → symbol) any kind goes.
    #[test]
    fn same_layer_sibling_links_are_uses() {
        use crate::LinkKind::*;
        let m = hex_model();
        assert_eq!(violation(&m, "app", "app2", Some(Uses)), None);
        let v = violation(&m, "app", "app2", Some(Calls)).unwrap();
        assert!(v.contains("joins two application components") && v.contains("`kind: uses`"), "{v}");
        assert_eq!(violation(&m, "dom", "dom_sym", Some(Depends)), None);
    }

    /// A link from another container lands on an inbound layer — never on the
    /// domain — and a link to the container itself is always fine.
    #[test]
    fn links_from_outside_land_on_inbound_layers() {
        use crate::LinkKind::*;
        let m = hex_model();
        assert_eq!(violation(&m, "page", "app", Some(Calls)), None);
        assert_eq!(violation(&m, "page", "pres", Some(Calls)), None);
        assert_eq!(violation(&m, "page", "svc", Some(Calls)), None);
        let v = violation(&m, "page", "dom", Some(Calls)).unwrap();
        assert!(v.contains("enters container 'Svc' (hexagonal) at 'Orders'"), "{v}");
        assert!(v.contains("land on presentation or application"), "{v}");
        let v = violation(&m, "ui", "infra", Some(Calls)).unwrap();
        assert!(v.contains("layer 'infrastructure'"), "{v}");
    }

    /// An application component nothing links into is dead; one driven by a
    /// presentation sibling or from outside is not. Presentation itself is the
    /// outermost inbound layer and is never asked to be reached.
    #[test]
    fn unreached_inner_inbound_components_are_flagged() {
        let mut m = hex_model();
        let link = |id: &str, src: &str, dst: &str| Link {
            id: id.into(), src: src.into(), dst: dst.into(),
            label: String::new(), method: None, kind: Some(crate::LinkKind::Calls),
        };
        m.links = vec![link("l1", "pres", "app"), link("l2", "page", "svc"), link("l0", "shell", "page")];
        let w = validate(&m);
        assert!(w.iter().any(|x| x.contains("Component app2 (\"Refunds\", application) in 'Svc' has nothing linking into it")), "{w:?}");
        assert!(!w.iter().any(|x| x.contains("Checkout") && x.contains("nothing linking")), "{w:?}");
        assert!(!w.iter().any(|x| x.contains("Http") && x.contains("nothing linking")), "{w:?}");
        // Reached from outside the container counts too.
        m.links.push(link("l3", "page", "app2"));
        let w = validate(&m);
        assert!(!w.iter().any(|x| x.contains("nothing linking")), "{w:?}");
    }

    /// Rule 5's proxy: four or more anchored components, one file each, no file
    /// shared → a file listing. A shared file or a two-file component breaks the
    /// signature.
    #[test]
    fn one_file_per_component_reads_as_a_file_listing() {
        let mut m = model(vec![
            serde_json::json!({ "id": "sys", "kind": "system", "name": "S" }),
            serde_json::json!({ "id": "c", "kind": "container", "name": "Core", "parentId": "sys", "style": "core-shell" }),
        ]);
        for i in 0..4 {
            m.nodes.push(node(serde_json::json!({
                "id": format!("k{i}"), "kind": "component", "name": format!("K{i}"), "parentId": "c", "layer": "core",
                "responsibilities": [{ "id": format!("r{i}"), "statement": "does x" }]
            })));
            m.source_map.insert(
                format!("r{i}"),
                vec![serde_json::from_value(serde_json::json!({ "pattern": format!("src/k{i}.rs") })).unwrap()],
            );
        }
        let w = validate(&m);
        assert!(w.iter().any(|x| x.contains("Container c (\"Core\") reads as a file listing") && x.contains("all 4 anchored")), "{w:?}");

        // One component spanning two files is a real cut → silent.
        m.source_map.get_mut("r0").unwrap().push(
            serde_json::from_value(serde_json::json!({ "pattern": "src/k0_extra.rs" })).unwrap(),
        );
        assert!(!validate(&m).iter().any(|x| x.contains("file listing")));
    }

    /// Inside a styled container every link says what it is; a container-level
    /// prose link does not have to.
    #[test]
    fn links_between_styled_nodes_need_a_kind() {
        let mut m = model(vec![
            serde_json::json!({ "id": "sys", "kind": "system", "name": "S" }),
            serde_json::json!({ "id": "c", "kind": "container", "name": "App", "parentId": "sys", "style": "core-shell" }),
            serde_json::json!({ "id": "d", "kind": "container", "name": "Db", "parentId": "sys", "style": "core-shell" }),
            serde_json::json!({ "id": "k1", "kind": "component", "name": "Cli", "parentId": "c", "layer": "shell" }),
            serde_json::json!({ "id": "k2", "kind": "component", "name": "Engine", "parentId": "c", "layer": "core" }),
        ]);
        let link = |id: &str, src: &str, dst: &str, kind| Link {
            id: id.into(),
            src: src.into(),
            dst: dst.into(),
            label: "uses".into(),
            method: None,
            kind,
        };
        m.links = vec![
            link("l1", "c", "d", None),
            link("l2", "k1", "k2", None),
            link("l3", "k2", "k1", Some(crate::LinkKind::Calls)),
        ];
        let w = validate(&m);
        assert!(w.iter().any(|x| x.contains("Link l2 (Cli → Engine) has no kind")), "{w:?}");
        assert!(!w.iter().any(|x| x.contains("Link l1")), "{w:?}");
        assert!(!w.iter().any(|x| x.contains("Link l3") && x.contains("kind")), "{w:?}");
    }
}

#[cfg(test)]
mod resp_id_tests {
    use super::validate;
    use crate::{Group, Node, ScryModel};

    fn node(v: serde_json::Value) -> Node {
        serde_json::from_value(v).unwrap()
    }
    fn group(v: serde_json::Value) -> Group {
        serde_json::from_value(v).unwrap()
    }

    /// A responsibility id living on two different hosts (here a node AND a group)
    /// breaks the global-uniqueness invariant the id minters and every id-keyed
    /// lookup rely on — the validator must flag it, naming every host.
    #[test]
    fn flags_a_responsibility_id_reused_across_hosts() {
        let mut m = ScryModel::new();
        m.nodes.push(node(serde_json::json!({
            "id": "node-1", "kind": "system", "name": "Acme",
            "responsibilities": [{ "id": "resp-1", "statement": "runs the show" }]
        })));
        m.nodes.push(node(serde_json::json!({
            "id": "node-2", "kind": "container", "name": "API", "parentId": "node-1",
            "responsibilities": [{ "id": "resp-1", "statement": "serves requests" }]
        })));
        m.groups.push(group(serde_json::json!({
            "id": "grp-1", "name": "Deploys as one", "parentNodeId": "node-1",
            "memberIds": ["node-2"],
            "responsibilities": [{ "id": "resp-1", "statement": "ships together" }]
        })));

        let warnings = validate(&m);
        let hits: Vec<&String> =
            warnings.iter().filter(|w| w.contains("globally unique")).collect();
        assert_eq!(hits.len(), 1, "exactly one collision warning, got: {warnings:?}");
        let w = hits[0];
        assert!(w.contains("resp-1"), "names the colliding id: {w}");
        assert!(
            w.contains("node-1") && w.contains("node-2") && w.contains("grp-1"),
            "names every host: {w}"
        );
    }

    /// The gating subset (`structural_violations`) catches the same-id-two-hosts
    /// collision AND plain duplicate node ids — the silent-misbind states the
    /// committed-write seam must refuse — while staying silent on advisories.
    #[test]
    fn structural_violations_flags_silent_misbind_states() {
        use super::structural_violations;
        let mut m = ScryModel::new();
        m.nodes.push(node(serde_json::json!({
            "id": "node-1", "kind": "system", "name": "Acme",
            "responsibilities": [{ "id": "resp-1", "statement": "a" }]
        })));
        // A second, DISTINCT host reusing the responsibility id — the move that
        // never cleaned up the old copy.
        m.nodes.push(node(serde_json::json!({
            "id": "node-2", "kind": "container", "name": "API", "parentId": "node-1",
            "responsibilities": [{ "id": "resp-1", "statement": "b" }]
        })));
        // A third node reusing node-2's id — a plain duplicate node identity.
        m.nodes.push(node(serde_json::json!({
            "id": "node-2", "kind": "container", "name": "Dup", "parentId": "node-1",
        })));
        let v = structural_violations(&m);
        assert!(v.iter().any(|w| w.contains("Duplicate node id: node-2")), "{v:?}");
        assert!(
            v.iter().any(|w| w.contains("globally unique") && w.contains("resp-1")),
            "{v:?}"
        );
    }

    /// A structurally sound model yields no gating violation — the seam only
    /// bites genuine invariant breaks, never ordinary (or advisory-flawed)
    /// content, so it can guard every committed write without false positives.
    #[test]
    fn structural_violations_empty_for_a_clean_model() {
        use super::structural_violations;
        let mut m = ScryModel::new();
        m.nodes.push(node(serde_json::json!({
            "id": "node-1", "kind": "system", "name": "Acme",
            "responsibilities": [{ "id": "resp-1", "statement": "a" }]
        })));
        m.nodes.push(node(serde_json::json!({
            "id": "node-2", "kind": "container", "name": "API", "parentId": "node-1",
            "responsibilities": [{ "id": "resp-2", "statement": "b" }]
        })));
        assert!(structural_violations(&m).is_empty(), "{:?}", structural_violations(&m));
    }

    /// Responsibility ids that are unique across hosts raise no collision warning.
    #[test]
    fn unique_responsibility_ids_pass() {
        let mut m = ScryModel::new();
        m.nodes.push(node(serde_json::json!({
            "id": "node-1", "kind": "system", "name": "Acme",
            "responsibilities": [{ "id": "resp-1", "statement": "runs the show" }]
        })));
        m.nodes.push(node(serde_json::json!({
            "id": "node-2", "kind": "container", "name": "API", "parentId": "node-1",
            "responsibilities": [{ "id": "resp-2", "statement": "serves requests" }]
        })));
        assert!(
            !validate(&m).iter().any(|w| w.contains("globally unique")),
            "distinct ids must not warn"
        );
    }

    /// A boundary glob with no directory prefix owns every otherwise-unowned
    /// file in the repo — flagged; a directory-rooted glob is not.
    #[test]
    fn flags_whole_repo_boundary_globs() {
        let mut m = ScryModel::new();
        m.nodes.push(node(serde_json::json!({
            "id": "node-1", "kind": "system", "name": "Acme",
        })));
        m.nodes.push(node(serde_json::json!({
            "id": "node-2", "kind": "container", "name": "API", "parentId": "node-1",
        })));
        m.boundaries.insert(
            "node-1".into(),
            vec![serde_json::from_value(serde_json::json!({ "pattern": "**/*" })).unwrap()],
        );
        m.boundaries.insert(
            "node-2".into(),
            vec![serde_json::from_value(serde_json::json!({ "pattern": "api/**/*" })).unwrap()],
        );

        let warnings = validate(&m);
        let hits: Vec<&String> =
            warnings.iter().filter(|w| w.contains("no directory prefix")).collect();
        assert_eq!(hits.len(), 1, "only the whole-repo glob warns: {warnings:?}");
        assert!(hits[0].contains("**/*") && hits[0].contains("Acme"), "{}", hits[0]);
    }
}

#[cfg(test)]
mod disconnect_tests {
    use super::validate;
    use crate::{Node, ScryModel};

    fn node(v: serde_json::Value) -> Node {
        serde_json::from_value(v).unwrap()
    }

    /// Component → Symbol scaffolding with `n` linkless sibling symbols under
    /// one component, each carrying a responsibility (so the empty-symbol
    /// warning stays out of the way).
    fn model_with_symbols(n: usize) -> ScryModel {
        let mut m = ScryModel::new();
        m.nodes.push(node(serde_json::json!({
            "id": "sys", "kind": "system", "name": "Acme",
        })));
        m.nodes.push(node(serde_json::json!({
            "id": "cont", "kind": "container", "name": "API", "parentId": "sys",
        })));
        m.nodes.push(node(serde_json::json!({
            "id": "comp", "kind": "component", "name": "Auth", "parentId": "cont",
        })));
        for i in 0..n {
            m.nodes.push(node(serde_json::json!({
                "id": format!("sym-{i}"), "kind": "symbol",
                "name": format!("helper_{i}"), "parentId": "comp",
                "responsibilities": [{ "id": format!("r-{i}"), "statement": "does a thing" }]
            })));
        }
        m
    }

    /// test_map keys must name a live responsibility — a test can only be
    /// attached to a claim that exists. (Node ids are not legal keys, unlike source_map's
    /// declaration anchors.)
    #[test]
    fn test_map_unknown_key_warns() {
        let mut m = model_with_symbols(1);
        m.test_map.insert(
            "r-0".into(),
            vec![serde_json::from_value(serde_json::json!({ "pattern": "tests/a.rs" })).unwrap()],
        );
        m.test_map.insert(
            "r-ghost".into(),
            vec![serde_json::from_value(serde_json::json!({ "pattern": "tests/b.rs" })).unwrap()],
        );
        let warnings = validate(&m);
        assert!(
            warnings.iter().any(|w| w.contains("Test map") && w.contains("r-ghost")),
            "unknown test key flagged: {warnings:?}"
        );
        assert!(
            !warnings.iter().any(|w| w.contains("Test map") && w.contains("'r-0'")),
            "a live claim's test entry is quiet: {warnings:?}"
        );
    }

    /// Symbols justify themselves through claims (rule 8) — a linkless symbol
    /// must not warn, however many siblings it has. The code-level dependency
    /// graph is legitimately sparse.
    #[test]
    fn linkless_symbols_never_warn() {
        let m = model_with_symbols(5);
        let warnings = validate(&m);
        assert!(
            !warnings.iter().any(|w| w.contains("disconnected") && w.contains("symbol")),
            "no per-symbol disconnect noise: {warnings:?}"
        );
    }

    /// A symbol whose links all render elsewhere still gets the (rarer,
    /// actionable) "links but none at this level" flag — only the bulk
    /// "no links" nag is dropped for symbols.
    #[test]
    fn symbol_with_invisible_links_still_warns() {
        let mut m = model_with_symbols(2);
        // Link sym-0 to the container: legal only via reference propagation,
        // but here nothing makes "API" a reference on the symbol view, so the
        // link renders nowhere at sym-0's level.
        m.links.push(serde_json::from_value(serde_json::json!({
            "id": "l1", "src": "sym-0", "dst": "cont", "label": "uses"
        })).unwrap());
        let warnings = validate(&m);
        assert!(
            warnings.iter().any(|w| w.contains("has links but none at this level")
                && w.contains("helper_0")),
            "invisible-link symbols still flagged: {warnings:?}"
        );
    }

    /// Linkless components roll up to ONE warning per view naming every
    /// culprit, not one line per node — a sparse diagram costs a line, not a
    /// wall the agent learns to skim.
    #[test]
    fn linkless_components_roll_up_per_view() {
        let mut m = ScryModel::new();
        m.nodes.push(node(serde_json::json!({
            "id": "sys", "kind": "system", "name": "Acme",
        })));
        m.nodes.push(node(serde_json::json!({
            "id": "cont", "kind": "container", "name": "API", "parentId": "sys",
        })));
        for (id, name) in [("c1", "Auth"), ("c2", "Billing"), ("c3", "Search")] {
            m.nodes.push(node(serde_json::json!({
                "id": id, "kind": "component", "name": name, "parentId": "cont",
            })));
        }
        let warnings = validate(&m);
        let hits: Vec<&String> = warnings
            .iter()
            .filter(|w| w.contains("have no links") || w.contains("has no links"))
            .collect();
        assert_eq!(hits.len(), 1, "one rolled-up line, got: {warnings:?}");
        let w = hits[0];
        assert!(
            w.contains("'Auth' (component)")
                && w.contains("'Billing' (component)")
                && w.contains("'Search' (component)"),
            "names every culprit with its kind: {w}"
        );
        assert!(w.contains("component view of 'API'"), "scoped to the view: {w}");
    }

    /// A single linkless node keeps the singular wording and gains the view
    /// scope; a lone node on its view (owned.len() == 1) still never warns.
    #[test]
    fn single_linkless_component_warns_singular_and_scoped() {
        let mut m = ScryModel::new();
        m.nodes.push(node(serde_json::json!({
            "id": "sys", "kind": "system", "name": "Acme",
        })));
        m.nodes.push(node(serde_json::json!({
            "id": "cont", "kind": "container", "name": "API", "parentId": "sys",
        })));
        m.nodes.push(node(serde_json::json!({
            "id": "c1", "kind": "component", "name": "Auth", "parentId": "cont",
        })));
        m.nodes.push(node(serde_json::json!({
            "id": "c2", "kind": "component", "name": "Billing", "parentId": "cont",
        })));
        m.links.push(serde_json::from_value(serde_json::json!({
            "id": "l1", "src": "c2", "dst": "c1", "label": "bills via"
        })).unwrap());
        // c1/c2 are connected; add a third that isn't.
        m.nodes.push(node(serde_json::json!({
            "id": "c3", "kind": "component", "name": "Search", "parentId": "cont",
        })));
        let warnings = validate(&m);
        let hits: Vec<&String> =
            warnings.iter().filter(|w| w.contains("has no links")).collect();
        assert_eq!(hits.len(), 1, "{warnings:?}");
        assert!(
            hits[0].contains("'Search' (component) has no links")
                && hits[0].contains("component view of 'API'"),
            "{}",
            hits[0]
        );
    }
}
