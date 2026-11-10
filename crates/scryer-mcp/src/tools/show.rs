//! Pointing the developer's open app at a node: the agent's way to show the human something while it works, or to
//! put a question's subject on screen before asking it in the session.
//!
//! Advertised only while the app has the project open (see [`ScryerServer::app_is_open`]): a tool the developer
//! can't see the effect of would only mislead the agent. A direction is always a notice the developer accepts: it
//! pauses whatever session they follow and takes them where the agent pointed, to talk about it in the session.

use crate::helpers::*;
use crate::server::ScryerServer;
use crate::tools::comments::resolve_on;
use crate::tools::read::load_or_err;
use crate::types::*;
use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, Content},
    tool, tool_router, ErrorData as McpError,
};
use scryer_core::app_view::{self, Direction};
use scryer_core::ScryModel;

fn error(text: impl Into<String>) -> Result<CallToolResult, McpError> {
    Ok(CallToolResult::error(vec![Content::text(text.into())]))
}

/// A claim statement as compared: display markup and case dropped.
fn plain(s: &str) -> String {
    s.replace("**", "").trim().trim_end_matches('.').to_lowercase()
}

/// Sort `flash` into the node's claims, its page fields and other nodes; an item naming none of them is an error.
fn sort_flash(
    model: &ScryModel,
    tree: &scryer_core::derived_tree::DerivedTree,
    node: &str,
    flash: &[String],
    d: &mut Direction,
) -> Result<(), String> {
    let claims = model.nodes.iter().find(|n| n.id == node).map(|n| n.claims.as_slice()).unwrap_or_default();
    for item in flash {
        let item = item.trim();
        if item.eq_ignore_ascii_case("description") {
            d.fields.push("description".into());
        } else if let Some(c) = claims.iter().find(|c| c.id == item || plain(&c.statement) == plain(item)) {
            d.claims.push(c.id.clone());
        } else {
            match resolve_on(model, tree, item) {
                Ok(id) => d.nodes.push(id),
                Err(_) => return Err(format!("'{item}' is no claim on the node, no node and not \"description\"")),
            }
        }
    }
    Ok(())
}

#[tool_router(router = tool_router_show, vis = "pub(crate)")]
impl ScryerServer {
    #[tool(
        description = "Point the developer's open scryer app at a node; the notice names your session. Use it when \
         something deserves their eyes: what you just changed, or what a question is about, before you ask it in the \
         session. Not every step. `node`: id, file or name. `view`: \"map\" or \"wiki\", default theirs. `flash`: \
         claims (id or statement), \"description\" or nodes to highlight."
    )]
    fn show(&self, Parameters(req): Parameters<ShowRequest>) -> Result<CallToolResult, McpError> {
        let model_ref = resolve_model_ref(req.project.as_deref())?;
        let project = model_ref.project_path();
        if let Some(v) = req.view.as_deref() {
            if v != "map" && v != "wiki" {
                return error(format!("`view` is \"map\" or \"wiki\", not '{v}'"));
            }
        }
        let loaded = match load_or_err(&model_ref) {
            Ok(l) => l,
            Err(e) => return Ok(e),
        };
        let model = &loaded.tree.planned;
        let node = match resolve_on(model, &loaded.tree, &req.node) {
            Ok(n) => n,
            Err(e) => return error(e),
        };
        let session = self.session_id(&model_ref);
        let sender = self.sender(&model_ref);
        let mut direction = Direction {
            session,
            sender,
            node: node.clone(),
            view: req.view.clone(),
            note: req.note.as_deref().map(str::trim).filter(|n| !n.is_empty()).map(str::to_string),
            at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or_default(),
            ..Default::default()
        };
        if let Err(e) = sort_flash(model, &loaded.tree, &node, &req.flash, &mut direction) {
            return error(e);
        }
        let name = model.nodes.iter().find(|n| n.id == node).map(|n| n.name.as_str()).unwrap_or(node.as_str());
        let link = format!("[{name}]({})", app_view::link_for(project, &direction));
        // Listed only while the app is open, so this is the app closing since: a race, not a state to explain.
        if app_view::app_open(project).is_none() {
            return error("The app closed; nothing was shown.");
        }
        if let Err(e) = app_view::write_direction(project, &direction) {
            return error(format!("Could not write the direction: {e}"));
        }
        let text = format!(
            "The app offers {name} to the developer in a notice; accepting it takes them there and pauses any follow. \
             Ask what you wanted to ask in the session. Its link, if you want one there: {link}"
        );
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::fixture::{project, project_arg, text_of};

    fn show(server: &ScryerServer, dir: &tempfile::TempDir, node: &str, view: Option<&str>, flash: &[&str]) -> CallToolResult {
        server
            .show(Parameters(ShowRequest {
                project: project_arg(dir),
                node: node.into(),
                view: view.map(str::to_string),
                flash: flash.iter().map(|s| s.to_string()).collect(),
                note: Some("  why save swallows errors ".into()),
            }))
            .unwrap()
    }

    /// When asked to show a node, writes a direction naming the node, the view, what to flash and the session's task.
    #[test]
    fn writes_a_direction_for_the_app() {
        let dir = project();
        app_view::mark_open(dir.path()).unwrap();
        let r = scryer_core::ModelRef::ProjectLocal(dir.path().to_path_buf());
        scryer_core::session_hooks::record_task(&r, "s-1", "fix the draft save").unwrap();
        let server = ScryerServer::for_session("s-1");
        let out = show(&server, &dir, "crates/notes/src/draft.rs", Some("wiki"), &["When a save fails, keeps the draft", "description", "crates/notes/src/store/mod.rs"]);
        assert_eq!(out.is_error, Some(false), "{}", text_of(&out));
        assert!(text_of(&out).starts_with("The app offers draft"), "{}", text_of(&out));

        let loaded = crate::derived::load(&r).unwrap();
        let model = &loaded.tree.planned;
        let draft = crate::derived::file_node(&loaded.tree, model, "crates/notes/src/draft.rs").unwrap();
        let store = crate::derived::file_node(&loaded.tree, model, "crates/notes/src/store/mod.rs").unwrap();
        let d = app_view::read_direction(dir.path()).unwrap();
        assert_eq!(d.node, draft.id);
        assert_eq!(d.view.as_deref(), Some("wiki"));
        assert_eq!(d.session.as_deref(), Some("s-1"));
        assert_eq!(d.sender, "fix the draft save");
        assert_eq!(d.note.as_deref(), Some("why save swallows errors"));
        let keeps = draft.claims.iter().find(|c| c.statement.contains("keeps the draft")).unwrap();
        assert_eq!(d.claims, vec![keeps.id.clone()]);
        assert_eq!(d.fields, vec!["description".to_string()]);
        assert_eq!(d.nodes, vec![store.id.clone()]);
    }

    /// If the node or an entry to flash names nothing in the tree, then refuses and writes nothing.
    #[test]
    fn refuses_what_names_nothing() {
        let dir = project();
        app_view::mark_open(dir.path()).unwrap();
        let server = ScryerServer::new();
        assert_eq!(show(&server, &dir, "nowhere", None, &[]).is_error, Some(true));
        let bad = show(&server, &dir, "crates/notes/src/draft.rs", None, &["no such claim"]);
        assert_eq!(bad.is_error, Some(true));
        assert!(text_of(&bad).contains("no such claim"), "{}", text_of(&bad));
        assert_eq!(show(&server, &dir, "crates/notes/src/draft.rs", Some("graph"), &[]).is_error, Some(true));
        assert_eq!(app_view::read_direction(dir.path()), None);
    }

    /// Says the direction is a notice the developer accepts, which pauses any follow.
    #[test]
    fn says_the_direction_is_a_notice_to_accept() {
        let dir = project();
        app_view::mark_open(dir.path()).unwrap();
        let out = show(&ScryerServer::new(), &dir, "crates/notes/src/draft.rs", None, &[]);
        assert_eq!(out.is_error, Some(false));
        assert!(text_of(&out).contains("accepting it takes them there and pauses any follow"), "{}", text_of(&out));
        assert!(app_view::read_direction(dir.path()).is_some());
    }

    /// If the app has closed since the tool was listed, then refuses and writes nothing.
    #[test]
    fn refuses_once_the_app_has_closed() {
        let dir = project();
        let out = show(&ScryerServer::new(), &dir, "crates/notes/src/draft.rs", None, &[]);
        assert_eq!(out.is_error, Some(true));
        assert_eq!(app_view::read_direction(dir.path()), None);
    }

    /// Gives with what it showed a markdown link that opens the app at the same place, view and flashes included.
    #[test]
    fn gives_a_link_to_what_it_showed() {
        let dir = project();
        app_view::mark_open(dir.path()).unwrap();
        let out = show(&ScryerServer::new(), &dir, "crates/notes/src/draft.rs", Some("wiki"), &["description"]);
        let text = text_of(&out);
        let url = text.split("](").nth(1).unwrap().trim_end_matches(')');
        let (project, d) = app_view::parse_link(url).unwrap();
        assert_eq!((project.as_path(), d.view.as_deref(), d.fields.as_slice()), (dir.path(), Some("wiki"), &["description".to_string()][..]));
    }
}
