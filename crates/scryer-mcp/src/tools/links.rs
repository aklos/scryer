use crate::helpers::*;
use crate::server::ScryerServer;
use crate::types::*;
use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, Content},
    tool, tool_router, ErrorData as McpError,
};
use scryer_core::Link;
use std::collections::HashSet;

#[tool_router(router = tool_router_links, vis = "pub(crate)")]
impl ScryerServer {
    #[tool(
        description = "Add links between nodes; direction is initiator (src) → provider (dst). Endpoints must \
         be siblings, or the deeper node's parent must already link to the other node — otherwise \
         the batch is rejected with guidance, so order parent-level links first. Also rejects \
         missing endpoints, self-loops, and ancestor↔descendant links. Returns the link ids.\n\
         Rules: links-same-level, one-link, mentions-imply-links"
    )]
    fn add_links(
        &self,
        Parameters(req): Parameters<AddLinkRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model_ref = resolve_model_ref(req.project.as_deref())?;
        let _lock = match lock_or_err(&model_ref) {
            Ok(l) => l,
            Err(e) => return Ok(e),
        };
        let mut model = match scryer_core::read_planned_seeded_at(&model_ref) {
            Ok(m) => m,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(read_fail("model", &model_ref, &e))]));
            }
        };

        let node_ids: HashSet<String> =
            model.nodes.iter().map(|n| n.id.clone()).collect();
        let mut added: Vec<String> = Vec::new();
        let mut reused: Vec<String> = Vec::new();
        for item in &req.links {
            if !node_ids.contains(&item.src) {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Unknown src node '{}'",
                    item.src
                ))]));
            }
            if !node_ids.contains(&item.dst) {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Unknown dst node '{}'",
                    item.dst
                ))]));
            }
            if item.src == item.dst {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Self-link rejected: {} -> {}",
                    item.src, item.dst
                ))]));
            }
            // Retry-safe: an IDENTICAL link (same endpoints and label) already
            // in the model is returned, not duplicated — a retried tool call
            // that mints a parallel copy is a workflow bug even though no
            // single call misbehaved.
            if let Some(existing) = model
                .links
                .iter()
                .find(|l| l.src == item.src && l.dst == item.dst && l.label == item.label)
            {
                reused.push(existing.id.clone());
                continue;
            }
            // Parallel edges (same endpoints, different labels) must NOT share
            // an id: both diff engines key links by id, so a collision merges
            // two links into one element — and folding then deletes both.
            let base = scryer_core::make_link_id(&item.src, &item.dst);
            let mut id = base.clone();
            let mut n = 2;
            while model.links.iter().any(|l| l.id == id) {
                id = format!("{base}-{n}");
                n += 1;
            }
            let link = Link {
                id: id.clone(),
                src: item.src.clone(),
                dst: item.dst.clone(),
                label: item.label.clone(),
                method: item.method.clone(),
            };
            model.links.push(link);
            added.push(id);
        }

        // Enforce the same-level / reference rule with every new link present,
        // so a batch may add a parent-level link and the child-level link that
        // depends on it together (order within the batch doesn't matter). Any
        // illegal link rejects the whole batch — nothing is written.
        let violations: Vec<String> = req
            .links
            .iter()
            .filter_map(|item| {
                scryer_core::validate::link_violation(&model, &item.src, &item.dst)
                    .map(|v| scryer_core::validate::describe_violation(&model, &item.src, &item.dst, &v))
            })
            .collect();
        if !violations.is_empty() {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "No links added — {} rejected:\n{}",
                violations.len(),
                violations.join("\n")
            ))]));
        }

        let tag_warnings = match write_planned_tagged(
            &model_ref,
            &mut model,
            self.session_change(&model_ref).as_deref(),
        ) {
            Ok(w) => w,
            Err(e) => return Ok(CallToolResult::error(vec![Content::text(e)])),
        };

        drop(_lock);
        let mut msg = format!(
            "Added {} link(s): {} — in the PLAN; a link folds when a whole-node fold makes \
             both endpoints committed, or explicitly via mark_implemented link_ids",
            added.len(),
            added.join(", ")
        );
        for w in &tag_warnings {
            msg.push_str(&format!("\n{w}"));
        }
        if !reused.is_empty() {
            msg.push_str(&format!(
                "\n{} identical link(s) already existed and were returned, not duplicated: {}",
                reused.len(),
                reused.join(", ")
            ));
        }
        if let Some(h) = status_header(&model_ref) {
            msg.push_str(&format!("\n{h}"));
        }
        Ok(CallToolResult::success(vec![Content::text(msg)]))
    }

    #[tool(
        description = "Patch one or more links by id. Only fields present change; a re-pointed link must still \
         be legal.\n\
         Rules: links-same-level, one-link"
    )]
    fn update_links(
        &self,
        Parameters(req): Parameters<UpdateLinkRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model_ref = resolve_model_ref(req.project.as_deref())?;
        let _lock = match lock_or_err(&model_ref) {
            Ok(l) => l,
            Err(e) => return Ok(e),
        };
        let mut model = match scryer_core::read_planned_seeded_at(&model_ref) {
            Ok(m) => m,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(read_fail("model", &model_ref, &e))]));
            }
        };

        let mut updated = 0usize;
        for u in &req.links {
            let Some(l) = model.links.iter_mut().find(|l| l.id == u.link_id) else {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Link '{}' not found",
                    u.link_id
                ))]));
            };
            if let Some(v) = &u.label {
                l.label = v.clone();
            }
            if let Some(v) = &u.method {
                // Empty string = CLEAR — method could be set but never removed.
                l.method = if v.is_empty() { None } else { Some(v.clone()) };
            }
            updated += 1;
        }

        let tag_warnings = match write_planned_tagged(
            &model_ref,
            &mut model,
            self.session_change(&model_ref).as_deref(),
        ) {
            Ok(w) => w,
            Err(e) => return Ok(CallToolResult::error(vec![Content::text(e)])),
        };

        drop(_lock);
        let mut msg = format!("Updated {} link(s)", updated);
        for w in &tag_warnings {
            msg.push_str(&format!("\n{w}"));
        }
        if let Some(h) = status_header(&model_ref) {
            msg.push_str(&format!("\n{h}"));
        }
        Ok(CallToolResult::success(vec![Content::text(msg)]))
    }

    #[tool(
        description = "Delete one or more links by id. Fold the deletion with mark_implemented `link_ids`.\n\
         Rules: fold-in-layers"
    )]
    fn delete_links(
        &self,
        Parameters(req): Parameters<DeleteLinkRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model_ref = resolve_model_ref(req.project.as_deref())?;
        let _lock = match lock_or_err(&model_ref) {
            Ok(l) => l,
            Err(e) => return Ok(e),
        };
        let mut model = match scryer_core::read_planned_seeded_at(&model_ref) {
            Ok(m) => m,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(read_fail("model", &model_ref, &e))]));
            }
        };

        let target: HashSet<&str> = req.link_ids.iter().map(|s| s.as_str()).collect();
        let before = model.links.len();
        model.links.retain(|l| !target.contains(l.id.as_str()));

        let tag_warnings = match write_planned_tagged(
            &model_ref,
            &mut model,
            self.session_change(&model_ref).as_deref(),
        ) {
            Ok(w) => w,
            Err(e) => return Ok(CallToolResult::error(vec![Content::text(e)])),
        };

        drop(_lock);
        let mut msg = format!(
            "Deleted {} link(s) — a link DELETION folds only via mark_implemented link_ids \
             (it never rides a node fold)",
            before - model.links.len()
        );
        for w in &tag_warnings {
            msg.push_str(&format!("\n{w}"));
        }
        if let Some(h) = status_header(&model_ref) {
            msg.push_str(&format!("\n{h}"));
        }
        Ok(CallToolResult::success(vec![Content::text(msg)]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scryer_core::{Kind, ModelRef, Node, ScryModel};

    fn node(id: &str, name: &str) -> Node {
        Node {
            id: id.into(),
            kind: Kind::Container,
            name: name.into(),
            vagrant: None,
            stale: None,
            parent_id: None,
            external: None,
            technology: None,
            description: None,
            responsibilities: Vec::new(),
            properties: Vec::new(),
            icon: None,
            notes: None,
            position: None,
            directives: Vec::new(),
        }
    }

    /// Parallel edges (same endpoints, different labels) mint DISTINCT ids —
    /// both diff engines key links by id, so a collision merged two links
    /// into one element and folding deleted both. And update_links clears
    /// `method` with an empty string (it could be set but never removed).
    #[test]
    fn parallel_links_get_distinct_ids_and_method_clears() {
        let dir = tempfile::tempdir().unwrap();
        let model_ref = ModelRef::ProjectLocal(dir.path().to_path_buf());
        let mut m = ScryModel::new();
        m.nodes.push(node("a", "A"));
        m.nodes.push(node("b", "B"));
        scryer_core::write_planned_at(&model_ref, &m).unwrap();
        let server = ScryerServer::with_change(dir.path());
        let project = dir.path().to_string_lossy().to_string();

        let r = server
            .add_links(Parameters(AddLinkRequest {
                project: Some(project.clone()),
                links: vec![
                    AddLinkItem {
                        src: "a".into(),
                        dst: "b".into(),
                        label: "reads from".into(),
                        method: Some("REST".into()),
                    },
                    AddLinkItem {
                        src: "a".into(),
                        dst: "b".into(),
                        label: "streams events to".into(),
                        method: None,
                    },
                ],
            }))
            .unwrap();
        assert!(!r.is_error.unwrap_or(false), "{r:?}");

        let after = scryer_core::read_planned_at(&model_ref).unwrap();
        assert_eq!(after.links.len(), 2);
        assert_ne!(after.links[0].id, after.links[1].id, "parallel edges stay distinct");

        // Clear the first link's method with an empty string.
        let first = after.links[0].id.clone();
        let r = server
            .update_links(Parameters(UpdateLinkRequest {
                project: Some(project),
                links: vec![UpdateLinkItem {
                    link_id: first.clone(),
                    label: None,
                    method: Some(String::new()),
                }],
            }))
            .unwrap();
        assert!(!r.is_error.unwrap_or(false), "{r:?}");
        let after = scryer_core::read_planned_at(&model_ref).unwrap();
        let l = after.links.iter().find(|l| l.id == first).unwrap();
        assert_eq!(l.method, None, "empty string cleared the method");
    }

    /// A retried add_links with an identical link (same endpoints and label)
    /// returns the existing id — it must not mint a parallel -2 copy. A
    /// genuinely different label on the same endpoints still creates the
    /// parallel edge.
    #[test]
    fn retried_add_links_reuses_identical_link() {
        let dir = tempfile::tempdir().unwrap();
        let model_ref = ModelRef::ProjectLocal(dir.path().to_path_buf());
        let mut m = ScryModel::new();
        m.nodes.push(node("a", "A"));
        m.nodes.push(node("b", "B"));
        scryer_core::write_planned_at(&model_ref, &m).unwrap();
        let server = ScryerServer::with_change(dir.path());
        let project = dir.path().to_string_lossy().to_string();
        let call = |label: &str| {
            server
                .add_links(Parameters(AddLinkRequest {
                    project: Some(project.clone()),
                    links: vec![AddLinkItem {
                        src: "a".into(),
                        dst: "b".into(),
                        label: label.into(),
                        method: None,
                    }],
                }))
                .unwrap()
        };
        call("queries");
        let text = serde_json::to_string(&call("queries").content).unwrap();
        assert!(
            text.contains("already existed"),
            "retry reuses the identical link: {text}"
        );
        let plan = scryer_core::read_planned_at(&model_ref).unwrap();
        assert_eq!(plan.links.len(), 1, "no parallel duplicate");

        // Different label = a real parallel edge, still allowed.
        call("streams to");
        let plan = scryer_core::read_planned_at(&model_ref).unwrap();
        assert_eq!(plan.links.len(), 2);
    }
}
