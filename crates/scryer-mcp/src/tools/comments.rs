//! Comment threads over MCP: what people and other agents asked of the
//! code, and the agent's replies. A thread closes only when the agent says
//! so with `addressed: true` — like closing an issue, never a side effect.

use crate::derived;
use crate::tools::read::load_or_err;
use crate::helpers::*;
use crate::server::ScryerServer;
use crate::types::*;
use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, Content},
    tool, tool_router, ErrorData as McpError,
};
use scryer_core::comments::{self, Author, Thread, ThreadStatus};
use scryer_core::ScryModel;

fn error(text: impl Into<String>) -> Result<CallToolResult, McpError> {
    Ok(CallToolResult::error(vec![Content::text(text.into())]))
}

/// The node `on` names: a node id, a project-relative file, or a node name
/// (exact, ignoring case; refused when two nodes share it).
fn resolve_on(model: &ScryModel, tree: &scryer_core::derived_tree::DerivedTree, on: &str) -> Result<String, String> {
    let on = on.trim();
    if model.nodes.iter().any(|n| n.id == on) {
        return Ok(on.to_string());
    }
    if let Some(n) = derived::file_node(tree, model, on) {
        return Ok(n.id.clone());
    }
    let named: Vec<_> = model.nodes.iter().filter(|n| n.name.eq_ignore_ascii_case(on)).collect();
    match named.as_slice() {
        [n] => Ok(n.id.clone()),
        [] => Err(format!("'{on}' names no node or file in the tree")),
        many => Err(format!(
            "'{on}' names {} nodes ({}); pass a node id or a file",
            many.len(),
            many.iter().map(|n| n.id.as_str()).collect::<Vec<_>>().join(", ")
        )),
    }
}

fn render(t: &Thread) -> serde_json::Value {
    serde_json::json!({
        "id": t.id,
        "on": t.anchor,
        "node": t.node,
        "status": match t.status { ThreadStatus::Open => "open", ThreadStatus::Addressed => "addressed" },
        "events": t.events.iter().map(|e| {
            let who = match e.author { Author::Human => "human", Author::Agent => "agent" };
            let mark = match e.status {
                Some(ThreadStatus::Addressed) => " [addressed]",
                Some(ThreadStatus::Open) => " [reopened]",
                None => "",
            };
            format!("{who}{mark}: {}", e.text)
        }).collect::<Vec<_>>(),
    })
}

#[tool_router(router = tool_router_comments, vis = "pub(crate)")]
impl ScryerServer {
    #[tool(
        description = "Comment threads people and other agents left on nodes, open ones first. `on` (a file, \
         node id or node name) narrows to that node and the nodes above it. An open thread is an ask: \
         answer it with `comment`, and mark it addressed only once it is done."
    )]
    fn comments(&self, Parameters(req): Parameters<CommentsRequest>) -> Result<CallToolResult, McpError> {
        let model_ref = resolve_model_ref(req.project.as_deref())?;
        let loaded = match load_or_err(&model_ref) {
            Ok(l) => l,
            Err(e) => return Ok(e),
        };
        let shown: Vec<&Thread> = match req.on.as_deref() {
            Some(on) => {
                let node = match resolve_on(&loaded.tree.planned, &loaded.tree, on) {
                    Ok(n) => n,
                    Err(e) => return error(e),
                };
                if req.include_addressed {
                    let open: Vec<&str> = loaded.open_threads_on(&node).iter().map(|t| t.id.as_str()).collect();
                    loaded.threads.iter().filter(|t| t.node == node || open.contains(&t.id.as_str())).collect()
                } else {
                    loaded.open_threads_on(&node)
                }
            }
            None => loaded
                .threads
                .iter()
                .filter(|t| req.include_addressed || t.status == ThreadStatus::Open)
                .collect(),
        };
        if shown.is_empty() {
            return Ok(CallToolResult::success(vec![Content::text("No open comment threads.")]));
        }
        let out: Vec<_> = shown.into_iter().map(render).collect();
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&out).unwrap_or_default(),
        )]))
    }

    #[tool(
        description = "Reply to a comment thread, or open one `on` a node to ask a person or another agent \
         something. `addressed: true` closes the thread: set it only when the ask is done, and say in \
         `text` what you did and where. `addressed: false` reopens one. A reply alone never closes a thread."
    )]
    fn comment(&self, Parameters(req): Parameters<CommentRequest>) -> Result<CallToolResult, McpError> {
        let model_ref = resolve_model_ref(req.project.as_deref())?;
        let project = model_ref.project_path();
        let status = req.addressed.map(|a| if a { ThreadStatus::Addressed } else { ThreadStatus::Open });
        let result = match (req.thread.as_deref(), req.on.as_deref()) {
            (Some(id), _) => comments::reply(project, id, Author::Agent, &req.text, status),
            (None, Some(on)) => {
                if status.is_some() {
                    return error("`addressed` belongs on a reply to an existing thread");
                }
                let loaded = match load_or_err(&model_ref) {
                    Ok(l) => l,
                    Err(e) => return Ok(e),
                };
                let model = &loaded.tree.planned;
                match resolve_on(model, &loaded.tree, on) {
                    Ok(node) => comments::open_thread(project, &node, &breadcrumb(model, &node), Author::Agent, &req.text),
                    Err(e) => Err(e),
                }
            }
            (None, None) => Err("pass `thread` to reply, or `on` to open a thread".into()),
        };
        match result {
            Ok(t) => Ok(CallToolResult::success(vec![Content::text(
                serde_json::to_string_pretty(&render(&t)).unwrap_or_default(),
            )])),
            Err(e) => error(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::fixture::{json_of, project, project_arg};
    use crate::tools::read::file_report;

    fn say(server: &ScryerServer, dir: &tempfile::TempDir, thread: Option<&str>, on: Option<&str>, text: &str, addressed: Option<bool>) -> CallToolResult {
        server
            .comment(Parameters(CommentRequest {
                project: project_arg(dir),
                thread: thread.map(str::to_string),
                on: on.map(str::to_string),
                text: text.into(),
                addressed,
            }))
            .unwrap()
    }

    /// When a thread is open on a file's node, the file's entry lists it; a reply keeps it open, and it closes only when marked addressed.
    #[test]
    fn a_thread_stays_open_until_explicitly_addressed() {
        let dir = project();
        let server = ScryerServer::new();
        let opened = json_of(&say(&server, &dir, None, Some("crates/notes/src/draft.rs"), "Why does save swallow errors?", None));
        let id = opened["id"].as_str().unwrap().to_string();
        assert_eq!(opened["status"], "open");

        let r = scryer_core::ModelRef::ProjectLocal(dir.path().to_path_buf());
        let loaded = derived::load(&r).unwrap();
        let peers = crate::tools::read::Peers::besides(&r, &loaded.tree, None);
        let entry = file_report(&loaded, &peers, "crates/notes/src/draft.rs", None);
        assert!(entry["comments"][0].as_str().unwrap().starts_with(&id), "{entry}");

        let replied = json_of(&say(&server, &dir, Some(&id), None, "Looking into it", None));
        assert_eq!(replied["status"], "open");
        let done = json_of(&say(&server, &dir, Some(&id), None, "save now returns the error; see draft.rs", Some(true)));
        assert_eq!(done["status"], "addressed");
        let loaded = derived::load(&r).unwrap();
        assert!(file_report(&loaded, &peers, "crates/notes/src/draft.rs", None).get("comments").is_none());
    }

    /// If `addressed` comes without a thread to reply to, then refuses it.
    #[test]
    fn addressed_needs_a_thread() {
        let dir = project();
        let server = ScryerServer::new();
        let r = say(&server, &dir, None, Some("crates/notes/src/draft.rs"), "done", Some(true));
        assert_eq!(r.is_error, Some(true));
    }
}
