//! Drift verdicts from the node page — surgical edits the user may make by
//! hand. The logic lives in `scryer_core::drift::verdicts`, shared with the
//! agent's `resolve_drift`; each command takes the model lock so the canvas
//! never races the agent's MCP writer.

fn locked(cwd: &str, f: impl FnOnce(&scryer_core::ModelRef) -> Result<(), String>) -> Result<(), String> {
    let model_ref = scryer_core::ModelRef::ProjectLocal(std::path::PathBuf::from(cwd));
    let _lock = scryer_core::lock_model(&model_ref)?;
    f(&model_ref)
}

#[tauri::command]
pub(crate) fn adopt_responsibility(cwd: String, resp_id: String) -> Result<(), String> {
    locked(&cwd, |r| scryer_core::drift::verdicts::adopt_responsibility(r, resp_id))
}

#[tauri::command]
pub(crate) fn reject_responsibility(cwd: String, resp_id: String) -> Result<(), String> {
    locked(&cwd, |r| scryer_core::drift::verdicts::reject_responsibility(r, resp_id))
}

#[tauri::command]
pub(crate) fn adopt_property(cwd: String, node_id: String, label: String) -> Result<(), String> {
    locked(&cwd, |r| scryer_core::drift::verdicts::adopt_property(r, node_id, label))
}

#[tauri::command]
pub(crate) fn reject_property(cwd: String, node_id: String, label: String) -> Result<(), String> {
    locked(&cwd, |r| scryer_core::drift::verdicts::reject_property(r, node_id, label))
}

#[tauri::command]
pub(crate) fn drop_responsibility(cwd: String, resp_id: String) -> Result<(), String> {
    locked(&cwd, |r| scryer_core::drift::verdicts::drop_responsibility(r, resp_id))
}

#[tauri::command]
pub(crate) fn reimplement_responsibility(cwd: String, resp_id: String) -> Result<(), String> {
    locked(&cwd, |r| scryer_core::drift::verdicts::reimplement_responsibility(r, resp_id))
}

#[tauri::command]
pub(crate) fn drop_property(cwd: String, node_id: String, label: String) -> Result<(), String> {
    locked(&cwd, |r| scryer_core::drift::verdicts::drop_property(r, node_id, label))
}

#[tauri::command]
pub(crate) fn reimplement_property(cwd: String, node_id: String, label: String) -> Result<(), String> {
    locked(&cwd, |r| scryer_core::drift::verdicts::reimplement_property(r, node_id, label))
}

#[tauri::command]
pub(crate) fn reword_responsibility(cwd: String, resp_id: String, statement: String) -> Result<(), String> {
    locked(&cwd, |r| scryer_core::drift::verdicts::reword_responsibility(r, resp_id, statement))
}

#[tauri::command]
pub(crate) fn drop_node(cwd: String, node_id: String) -> Result<(), String> {
    locked(&cwd, |r| scryer_core::drift::verdicts::drop_node(r, node_id))
}

#[tauri::command]
pub(crate) fn reimplement_node(cwd: String, node_id: String) -> Result<(), String> {
    locked(&cwd, |r| scryer_core::drift::verdicts::reimplement_node(r, node_id))
}

#[cfg(test)]
mod verdict_tests {
    use scryer_core::{ModelRef, ScryModel};

    fn node(v: serde_json::Value) -> scryer_core::Node {
        serde_json::from_value(v).unwrap()
    }

    fn resp(id: &str, statement: &str, flags: &str) -> scryer_core::Responsibility {
        scryer_core::Responsibility {
            concern: None,
            id: id.into(),
            statement: statement.into(),
            vagrant: (flags == "vagrant").then_some(true),
            stale: (flags == "stale").then_some(true),
            stale_proposal: None,
            directives: Vec::new(),
            last_touched_at: None,
        }
    }

    fn prop(label: &str, flags: &str) -> scryer_core::SchemaProperty {
        serde_json::from_value(serde_json::json!({
            "label": label,
            "vagrant": (flags == "vagrant").then_some(true),
            "stale": (flags == "stale").then_some(true),
        }))
        .unwrap()
    }

    fn anchor(file: &str) -> Vec<scryer_core::SourceLocation> {
        vec![serde_json::from_value(serde_json::json!({ "pattern": file })).unwrap()]
    }

    /// system node-1 → container node-2 → symbol node-3 carrying `resp-1`
    /// (anchored). Both layers identical to start.
    fn base_model() -> ScryModel {
        let mut m = ScryModel::new();
        m.nodes.push(node(serde_json::json!({ "id": "node-1", "kind": "system", "name": "Acme" })));
        m.nodes.push(node(
            serde_json::json!({ "id": "node-2", "kind": "container", "name": "API", "parentId": "node-1" }),
        ));
        let mut sym = node(
            serde_json::json!({ "id": "node-3", "kind": "symbol", "name": "alpha", "parentId": "node-2" }),
        );
        sym.responsibilities = vec![resp("resp-1", "does alpha", "")];
        m.nodes.push(sym);
        m.source_map.insert("resp-1".into(), anchor("src/alpha.ts"));
        m
    }

    fn project(committed: &ScryModel, planned: &ScryModel) -> (tempfile::TempDir, ModelRef, String) {
        let dir = tempfile::tempdir().unwrap();
        let r = ModelRef::ProjectLocal(dir.path().to_path_buf());
        scryer_core::write_model_at(&r, committed).unwrap();
        scryer_core::write_planned_at(&r, planned).unwrap();
        let cwd = dir.path().to_string_lossy().to_string();
        (dir, r, cwd)
    }

    fn find_resp<'a>(m: &'a ScryModel, id: &str) -> Option<&'a scryer_core::Responsibility> {
        m.nodes.iter().flat_map(|n| &n.responsibilities).find(|r| r.id == id)
    }

    /// A plan with a code-discovered claim on a freshly MINTED symbol: adopting
    /// walks the vagrant chain up to the committed container, clears every
    /// vagrant flag, and folds chain + claim into committed.
    #[test]
    fn adopting_a_vagrant_claim_folds_its_minted_chain_into_committed() {
        let committed = base_model();
        let mut planned = base_model();
        let mut minted = node(serde_json::json!({
            "id": "node-9", "kind": "symbol", "name": "beta", "parentId": "node-2", "vagrant": true
        }));
        minted.responsibilities = vec![resp("resp-9", "does beta", "vagrant")];
        planned.nodes.push(minted);
        let (_dir, r, cwd) = project(&committed, &planned);

        super::adopt_responsibility(cwd, "resp-9".into()).unwrap();

        let committed = scryer_core::read_model_at(&r).unwrap();
        let host = committed.nodes.iter().find(|n| n.id == "node-9").expect("chain folded");
        assert_eq!(host.vagrant, None, "minted rung folds clean");
        let r9 = find_resp(&committed, "resp-9").expect("claim folded");
        assert_eq!(r9.vagrant, None, "adopted claim folds clean");
        let planned = scryer_core::read_planned_at(&r).unwrap();
        assert_eq!(find_resp(&planned, "resp-9").unwrap().vagrant, None);
    }

    /// Rejecting folds the claim into committed too — then removes it (and its
    /// minted chain) from the plan, so the diff reads as a deletion to-do.
    #[test]
    fn rejecting_a_vagrant_claim_leaves_a_deletion_todo() {
        let committed = base_model();
        let mut planned = base_model();
        let mut minted = node(serde_json::json!({
            "id": "node-9", "kind": "symbol", "name": "beta", "parentId": "node-2", "vagrant": true
        }));
        minted.responsibilities = vec![resp("resp-9", "does beta", "vagrant")];
        planned.nodes.push(minted);
        let (_dir, r, cwd) = project(&committed, &planned);

        super::reject_responsibility(cwd, "resp-9".into()).unwrap();

        let committed = scryer_core::read_model_at(&r).unwrap();
        assert!(find_resp(&committed, "resp-9").is_some(), "committed describes it");
        let planned = scryer_core::read_planned_at(&r).unwrap();
        assert!(find_resp(&planned, "resp-9").is_none(), "plan schedules the deletion");
        assert!(!planned.nodes.iter().any(|n| n.id == "node-9"), "minted chain leaves the plan");
    }

    /// Adopting a code-discovered field folds it (vagrant cleared) into
    /// committed — the field already exists in code.
    #[test]
    fn adopting_a_vagrant_property_folds_the_field() {
        let committed = base_model();
        let mut planned = base_model();
        planned.nodes[2].properties.push(prop("phone", "vagrant"));
        let (_dir, r, cwd) = project(&committed, &planned);

        super::adopt_property(cwd, "node-3".into(), "phone".into()).unwrap();

        let committed = scryer_core::read_model_at(&r).unwrap();
        let p = committed.nodes[2].properties.iter().find(|p| p.label == "phone").unwrap();
        assert_eq!(p.vagrant, None);
        let planned = scryer_core::read_planned_at(&r).unwrap();
        assert_eq!(
            planned.nodes[2].properties.iter().find(|p| p.label == "phone").unwrap().vagrant,
            None
        );
    }

    /// Rejecting a code-discovered field folds it into committed, then drops it
    /// from the plan — a deletion to-do.
    #[test]
    fn rejecting_a_vagrant_property_schedules_its_deletion() {
        let committed = base_model();
        let mut planned = base_model();
        planned.nodes[2].properties.push(prop("phone", "vagrant"));
        let (_dir, r, cwd) = project(&committed, &planned);

        super::reject_property(cwd, "node-3".into(), "phone".into()).unwrap();

        let committed = scryer_core::read_model_at(&r).unwrap();
        assert!(committed.nodes[2].properties.iter().any(|p| p.label == "phone"));
        let planned = scryer_core::read_planned_at(&r).unwrap();
        assert!(!planned.nodes[2].properties.iter().any(|p| p.label == "phone"));
    }

    /// Dropping a stale claim removes it from BOTH layers and GCs its anchor.
    #[test]
    fn dropping_a_stale_claim_removes_it_from_both_layers() {
        let mut committed = base_model();
        committed.nodes[2].responsibilities[0].stale = Some(true);
        let planned = committed.clone();
        let (_dir, r, cwd) = project(&committed, &planned);

        super::drop_responsibility(cwd, "resp-1".into()).unwrap();

        let committed = scryer_core::read_model_at(&r).unwrap();
        let planned = scryer_core::read_planned_at(&r).unwrap();
        assert!(find_resp(&committed, "resp-1").is_none());
        assert!(find_resp(&planned, "resp-1").is_none());
        assert!(!committed.source_map.contains_key("resp-1"), "anchor GC'd");
        assert!(!planned.source_map.contains_key("resp-1"));
    }

    /// Re-implementing a stale claim removes it from committed while the plan
    /// keeps a clean, anchored to-do.
    #[test]
    fn reimplementing_a_stale_claim_keeps_an_anchored_todo_in_the_plan() {
        let mut committed = base_model();
        committed.nodes[2].responsibilities[0].stale = Some(true);
        let planned = committed.clone();
        let (_dir, r, cwd) = project(&committed, &planned);

        super::reimplement_responsibility(cwd, "resp-1".into()).unwrap();

        let committed = scryer_core::read_model_at(&r).unwrap();
        assert!(find_resp(&committed, "resp-1").is_none(), "committed only holds satisfied claims");
        let planned = scryer_core::read_planned_at(&r).unwrap();
        let todo = find_resp(&planned, "resp-1").expect("the plan keeps the to-do");
        assert_eq!(todo.stale, None, "clean to-do, not drift");
        assert!(planned.source_map.contains_key("resp-1"), "to-do stays anchored");
    }

    /// Dropping a stale field removes it from both layers entirely.
    #[test]
    fn dropping_a_stale_field_removes_it_from_both_layers() {
        let mut committed = base_model();
        committed.nodes[2].properties.push(prop("phone", "stale"));
        let planned = committed.clone();
        let (_dir, r, cwd) = project(&committed, &planned);

        super::drop_property(cwd, "node-3".into(), "phone".into()).unwrap();

        let committed = scryer_core::read_model_at(&r).unwrap();
        let planned = scryer_core::read_planned_at(&r).unwrap();
        assert!(!committed.nodes[2].properties.iter().any(|p| p.label == "phone"));
        assert!(!planned.nodes[2].properties.iter().any(|p| p.label == "phone"));
    }

    /// Re-implementing a stale field removes it from committed while the plan
    /// keeps a clean to-do.
    #[test]
    fn reimplementing_a_stale_field_keeps_a_todo_in_the_plan() {
        let mut committed = base_model();
        committed.nodes[2].properties.push(prop("phone", "stale"));
        let planned = committed.clone();
        let (_dir, r, cwd) = project(&committed, &planned);

        super::reimplement_property(cwd, "node-3".into(), "phone".into()).unwrap();

        let committed = scryer_core::read_model_at(&r).unwrap();
        assert!(!committed.nodes[2].properties.iter().any(|p| p.label == "phone"));
        let planned = scryer_core::read_planned_at(&r).unwrap();
        let p = planned.nodes[2].properties.iter().find(|p| p.label == "phone").unwrap();
        assert_eq!(p.stale, None);
    }

    /// Accepting a reworded statement applies it to BOTH layers and clears the
    /// stale/proposal flags — the layers stay identical, no plan work item.
    #[test]
    fn accepting_a_reword_lands_in_both_layers_and_clears_the_flags() {
        let mut committed = base_model();
        committed.nodes[2].responsibilities[0].stale = Some(true);
        committed.nodes[2].responsibilities[0].stale_proposal = Some("does alpha, revised".into());
        let planned = committed.clone();
        let (_dir, r, cwd) = project(&committed, &planned);

        super::reword_responsibility(cwd, "resp-1".into(), "does alpha, revised".into()).unwrap();

        for m in [scryer_core::read_model_at(&r).unwrap(), scryer_core::read_planned_at(&r).unwrap()] {
            let r1 = find_resp(&m, "resp-1").unwrap();
            assert_eq!(r1.statement, "does alpha, revised");
            assert_eq!(r1.stale, None);
            assert_eq!(r1.stale_proposal, None);
        }
    }

    /// Dropping a stale node removes the whole subtree — claims, links,
    /// anchors, boundaries — from both layers.
    #[test]
    fn dropping_a_stale_node_removes_the_subtree_everywhere() {
        let mut committed = base_model();
        committed.nodes.push(node(
            serde_json::json!({ "id": "node-4", "kind": "symbol", "name": "gamma", "parentId": "node-1" }),
        ));
        committed.links.push(
            serde_json::from_value(
                serde_json::json!({ "id": "link-1", "src": "node-3", "dst": "node-4", "label": "calls" }),
            )
            .unwrap(),
        );
        committed
            .boundaries
            .insert("node-2".into(), vec![serde_json::from_value(serde_json::json!({ "pattern": "src/**/*" })).unwrap()]);
        let planned = committed.clone();
        let (_dir, r, cwd) = project(&committed, &planned);

        super::drop_node(cwd, "node-2".into()).unwrap();

        for m in [scryer_core::read_model_at(&r).unwrap(), scryer_core::read_planned_at(&r).unwrap()] {
            assert!(!m.nodes.iter().any(|n| n.id == "node-2" || n.id == "node-3"), "subtree gone");
            assert!(m.nodes.iter().any(|n| n.id == "node-4"), "outsiders survive");
            assert!(m.links.is_empty(), "links touching the subtree go with it");
            assert!(!m.source_map.contains_key("resp-1"), "descendant anchors GC'd");
            assert!(!m.boundaries.contains_key("node-2"));
        }
    }

    /// Re-implementing a stale node removes its subtree from committed while
    /// the plan keeps it as a clean to-do.
    #[test]
    fn reimplementing_a_stale_node_keeps_the_subtree_as_a_plan_todo() {
        let mut committed = base_model();
        committed.nodes[2].stale = Some(true);
        committed.nodes[2].responsibilities[0].stale = Some(true);
        let planned = committed.clone();
        let (_dir, r, cwd) = project(&committed, &planned);

        super::reimplement_node(cwd, "node-3".into()).unwrap();

        let committed = scryer_core::read_model_at(&r).unwrap();
        assert!(!committed.nodes.iter().any(|n| n.id == "node-3"), "subtree leaves committed");
        let planned = scryer_core::read_planned_at(&r).unwrap();
        let kept = planned.nodes.iter().find(|n| n.id == "node-3").expect("plan keeps the to-do");
        assert_eq!(kept.stale, None);
        assert_eq!(kept.responsibilities[0].stale, None, "claims read as clean pending work");
    }
}

#[cfg(test)]
mod plan_seed_tests {
    use scryer_core::{ModelRef, ScryModel};

    fn resp(id: &str, statement: &str) -> scryer_core::Responsibility {
        scryer_core::Responsibility {
            concern: None,
            id: id.into(),
            statement: statement.into(),
            vagrant: None,
            stale: None,
            stale_proposal: None,
            directives: Vec::new(),
            last_touched_at: None,
        }
    }

    /// Regression for the plan-seed seam (audit theme 1): a canvas verdict on a
    /// project with a COMMITTED model but no `planned.scry` yet must seed a CLEAN
    /// draft, not persist the committed fallback (anchors and all) as the plan.
    /// Without the seed, `read_planned_at` returns committed and writing it back
    /// mints the draft as a full shadow of committed's source_map/boundaries.
    #[test]
    fn a_verdict_seeds_a_clean_plan_without_shadowing_committed_anchors() {
        let dir = tempfile::tempdir().unwrap();
        let r = ModelRef::ProjectLocal(dir.path().to_path_buf());

        // Committed: a container whose responsibility carries a source anchor and
        // whose box carries a boundary glob. NO planned.scry is written.
        let node = |v: serde_json::Value| serde_json::from_value::<scryer_core::Node>(v).unwrap();
        let mut model = ScryModel::new();
        model
            .nodes
            .push(node(serde_json::json!({ "id": "node-1", "kind": "system", "name": "Acme" })));
        let mut cont = node(serde_json::json!({
            "id": "node-2", "kind": "container", "name": "API", "parentId": "node-1"
        }));
        cont.responsibilities = vec![resp("resp-1", "serves the API")];
        model.nodes.push(cont);
        model.source_map.insert(
            "resp-1".into(),
            vec![serde_json::from_value(serde_json::json!({ "pattern": "api/mod.rs" })).unwrap()],
        );
        model.boundaries.insert(
            "node-2".into(),
            vec![serde_json::from_value(serde_json::json!({ "pattern": "api/**/*" })).unwrap()],
        );
        scryer_core::write_model_at(&r, &model).unwrap();
        assert!(!r.planned_path().exists(), "precondition: no draft exists yet");

        // A canvas verdict with no prior draft (reword lands in both layers).
        let cwd = dir.path().to_string_lossy().to_string();
        super::reword_responsibility(cwd, "resp-1".into(), "serves the public API".into()).unwrap();

        // The draft now exists and carries the reworded claim, but owns NO shadow
        // of committed's anchors: a committed element's mapping lives in committed
        // alone.
        let plan = scryer_core::read_planned_at(&r).unwrap();
        let r1 = plan
            .nodes
            .iter()
            .flat_map(|n| &n.responsibilities)
            .find(|x| x.id == "resp-1")
            .unwrap();
        assert_eq!(r1.statement, "serves the public API", "the verdict landed in the plan");
        assert!(plan.source_map.is_empty(), "draft must not shadow committed's source_map");
        assert!(plan.boundaries.is_empty(), "draft must not shadow committed's boundaries");

        // Committed keeps its anchors — nothing was moved or lost.
        let committed = scryer_core::read_model_at(&r).unwrap();
        assert!(committed.source_map.contains_key("resp-1"));
        assert!(committed.boundaries.contains_key("node-2"));
    }

}
