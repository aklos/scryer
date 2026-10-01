//! Deterministic structural seeding (A2): mint the system + container skeleton
//! of a model from discovered deployable-unit facts, with no agent involved.
//! This is the structure the build previously created in a blocking agent
//! session; minting it mechanically at t=0 puts the skeleton on the canvas
//! instantly and lets every semantic session start concurrently.

use crate::{next_link_id, next_node_id, Kind, Link, Node, ScryModel};

/// The manifest facts one deployable unit contributes to the seed — a plain
/// projection of `scryer-extract`'s `ContainerFacts`, redeclared here so the
/// model crate stays independent of the extractor.
#[derive(Debug, Clone)]
pub struct SeedUnit {
    /// Directory relative to the project root, `/`-separated. Empty = root.
    pub dir: String,
    pub name: String,
    /// Literal declared technology (e.g. a Cargo edition, a Dockerfile base).
    pub technology: Option<String>,
    /// Directories of other units this one declares a path dependency on.
    pub dep_dirs: Vec<String>,
}

/// Mint the structural skeleton: the system node, one Container per unit (raw
/// manifest name + declared technology), its boundary glob, and unlabeled
/// container→container links along declared path dependencies.
///
/// Idempotent against an existing model: the system node is reused when
/// present, containers are matched by boundary-glob dir (falling back to exact
/// name only for a container that has no boundary dir), and links are only
/// added between fresh pairs — so a re-run never duplicates.
/// Returns the system id and one `(node_id, name, dir)` triple per unit, in
/// input order.
pub fn mint_initial_structure(
    model: &mut ScryModel,
    project_name: &str,
    units: &[SeedUnit],
) -> (String, Vec<(String, String, String)>) {
    let mk_node = |id: &str, kind: &str, name: &str, parent: Option<&str>, tech: &Option<String>| {
        serde_json::from_value::<Node>(serde_json::json!({
            "id": id, "kind": kind, "name": name, "parentId": parent, "technology": tech,
        }))
        .expect("static node shape")
    };

    let system_id = match model
        .nodes
        .iter()
        .find(|n| n.kind == Kind::System && n.external != Some(true))
    {
        Some(n) => n.id.clone(),
        None => {
            let id = next_node_id(model);
            model
                .nodes
                .push(mk_node(&id, "system", project_name, None, &None));
            id
        }
    };

    // The boundary-glob directory a node round-trips to, if it carries one.
    // Inverts the glob this function mints: `{dir}/**/*`, or the bare `**/*`
    // for a root unit (dir == "").
    let node_dir = |model: &ScryModel, n: &Node| -> Option<String> {
        model.boundaries.get(&n.id).and_then(|s| s.first()).map(|s| {
            if s.pattern == "**/*" {
                return String::new();
            }
            s.pattern
                .trim_end_matches("/**/*")
                .trim_end_matches("/**")
                .trim_end_matches("/*")
                .to_string()
        })
    };

    let mut triples: Vec<(String, String, String)> = Vec::new();
    for unit in units {
        let existing = model
            .nodes
            .iter()
            .find(|n| {
                n.kind == Kind::Container
                    && n.external != Some(true)
                    // A unit's identity is its directory. Match by boundary
                    // dir; fall back to name ONLY for a container with no
                    // boundary dir to compare against (e.g. one hand-added).
                    // Never let name alone match a container whose dir differs:
                    // two units that merely share a manifest name (a Tauri
                    // app's `src/` and `src-tauri/` are both "scryer") are
                    // distinct deployable units and earn distinct containers.
                    && match node_dir(model, n) {
                        Some(dir) => dir == unit.dir,
                        None => !unit.name.is_empty() && n.name == unit.name,
                    }
            })
            .map(|n| (n.id.clone(), n.name.clone()));
        let (id, name) = match existing {
            Some(pair) => pair,
            None => {
                let id = next_node_id(model);
                model.nodes.push(mk_node(
                    &id,
                    "container",
                    &unit.name,
                    Some(&system_id),
                    &unit.technology,
                ));
                let glob = if unit.dir.is_empty() {
                    "**/*".to_string()
                } else {
                    format!("{}/**/*", unit.dir)
                };
                model.boundaries.insert(
                    id.clone(),
                    vec![serde_json::from_value(serde_json::json!({ "pattern": glob }))
                        .expect("static source shape")],
                );
                (id, unit.name.clone())
            }
        };
        triples.push((id, name, unit.dir.clone()));
    }

    // Declared path dependencies become unlabeled container links; the system
    // semantic session labels them. Skip pairs already linked either way.
    let id_by_dir: std::collections::HashMap<&str, &str> = units
        .iter()
        .zip(&triples)
        .map(|(unit, (id, _, _))| (unit.dir.as_str(), id.as_str()))
        .collect();
    for unit in units {
        let Some(&src) = id_by_dir.get(unit.dir.as_str()) else { continue };
        for dep in &unit.dep_dirs {
            let Some(&dst) = id_by_dir.get(dep.as_str()) else { continue };
            let exists = model
                .links
                .iter()
                .any(|l| (l.src == src && l.dst == dst) || (l.src == dst && l.dst == src));
            if src != dst && !exists {
                let id = next_link_id(model);
                model.links.push(Link {
                    kind: None,
                    id,
                    src: src.to_string(),
                    dst: dst.to_string(),
                    label: String::new(),
                    method: None,
                });
            }
        }
    }

    (system_id, triples)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn units() -> Vec<SeedUnit> {
        vec![
            SeedUnit {
                dir: "".into(),
                name: "app".into(),
                technology: Some("Vite".into()),
                dep_dirs: vec!["crates/core".into()],
            },
            SeedUnit {
                dir: "crates/core".into(),
                name: "core".into(),
                technology: Some("Rust".into()),
                dep_dirs: vec![],
            },
        ]
    }

    #[test]
    fn mints_system_containers_boundaries_and_dep_links() {
        let mut model = ScryModel::new();
        let (system_id, triples) = mint_initial_structure(&mut model, "proj", &units());

        let system = model.nodes.iter().find(|n| n.id == system_id).unwrap();
        assert_eq!(system.kind, Kind::System);
        assert_eq!(triples.len(), 2);
        for (id, _, dir) in &triples {
            let node = model.nodes.iter().find(|n| &n.id == id).unwrap();
            assert_eq!(node.kind, Kind::Container);
            assert_eq!(node.parent_id.as_deref(), Some(system_id.as_str()));
            let glob = &model.boundaries.get(id).unwrap()[0].pattern;
            let expected = if dir.is_empty() {
                "**/*".to_string()
            } else {
                format!("{dir}/**/*")
            };
            assert_eq!(glob, &expected);
        }
        // app depends on crates/core → one unlabeled link.
        assert_eq!(model.links.len(), 1);
        assert_eq!(model.links[0].src, triples[0].0);
        assert_eq!(model.links[0].dst, triples[1].0);
        assert!(model.links[0].label.is_empty());
    }

    #[test]
    fn same_named_units_in_different_dirs_mint_distinct_containers() {
        // A Tauri app's frontend and backend both report manifest name
        // "scryer" but live in different dirs — they must NOT collapse into one
        // container (which would schedule two Wave-2 sessions for one node).
        let units = vec![
            SeedUnit { dir: "".into(), name: "scryer".into(), technology: None, dep_dirs: vec![] },
            SeedUnit {
                dir: "src-tauri".into(),
                name: "scryer".into(),
                technology: None,
                dep_dirs: vec![],
            },
        ];
        let mut model = ScryModel::new();
        let (_system_id, triples) = mint_initial_structure(&mut model, "scryer", &units);

        assert_eq!(triples.len(), 2);
        assert_ne!(triples[0].0, triples[1].0, "distinct dirs must mint distinct container ids");
        assert_eq!(model.nodes.iter().filter(|n| n.kind == Kind::Container).count(), 2);
        // Re-running discovery on the same dirs stays idempotent.
        let (_s, triples_again) = mint_initial_structure(&mut model, "scryer", &units);
        assert_eq!(triples, triples_again);
        assert_eq!(model.nodes.iter().filter(|n| n.kind == Kind::Container).count(), 2);
    }

    #[test]
    fn reminting_is_idempotent() {
        let mut model = ScryModel::new();
        let (sys_a, triples_a) = mint_initial_structure(&mut model, "proj", &units());
        let nodes_after_first = model.nodes.len();
        let (sys_b, triples_b) = mint_initial_structure(&mut model, "proj", &units());
        assert_eq!(sys_a, sys_b);
        assert_eq!(triples_a, triples_b);
        assert_eq!(model.nodes.len(), nodes_after_first);
        assert_eq!(model.links.len(), 1);
    }
}
