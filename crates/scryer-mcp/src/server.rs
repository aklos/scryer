use crate::instructions::INSTRUCTIONS;
use rmcp::{
    handler::server::router::tool::ToolRouter,
    model::{InitializeRequestParams, InitializeResult, ServerCapabilities, ServerInfo},
    service::{RequestContext, RoleServer},
    tool_handler, ServerHandler,
};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct ScryerServer {
    tool_router: ToolRouter<Self>,
    /// The change this SESSION is writing into (set via `set_change`), scoped
    /// to the project it was opened in. Deliberately in-memory only: the
    /// ledger itself (registry + tags) is persisted in the plan, but "which
    /// change am I writing to" is a per-session pointer — a fresh session sees
    /// the open changes and re-selects, it does not inherit a stale one. The
    /// server is stdio, one process per agent session, so process state IS
    /// session state.
    current_change: Arc<Mutex<Option<(PathBuf, String)>>>,
}

impl ScryerServer {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router_read()
                + Self::tool_router_nodes()
                + Self::tool_router_links()
                + Self::tool_router_misc()
                + Self::tool_router_generation()
                + Self::tool_router_intent(),
            current_change: Arc::new(Mutex::new(None)),
        }
    }

    /// The session's current change id, if one is set FOR THIS PROJECT — a
    /// change opened in project A never tags writes into project B.
    pub(crate) fn session_change(&self, model_ref: &scryer_core::ModelRef) -> Option<String> {
        let cur = self.current_change.lock().ok()?;
        let (project, id) = cur.as_ref()?;
        (project == model_ref.project_path()).then(|| id.clone())
    }

    pub(crate) fn set_session_change(&self, value: Option<(PathBuf, String)>) {
        if let Ok(mut cur) = self.current_change.lock() {
            *cur = value;
        }
    }
}

#[tool_handler]
impl ServerHandler for ScryerServer {
    fn get_info(&self) -> ServerInfo {
        // The connect-time block is kept tight and imperative on purpose: it is
        // always-loaded context, so it leads with the working loop and points at
        // `get_rules` for the rule text rather than inlining the rules index.
        ServerInfo {
            instructions: Some(INSTRUCTIONS.into()),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }

    fn initialize(
        &self,
        request: InitializeRequestParams,
        context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<InitializeResult, rmcp::ErrorData>> + Send + '_ {
        if context.peer.peer_info().is_none() {
            context.peer.set_peer_info(request);
        }
        std::future::ready(Ok(self.get_info()))
    }
}
