use crate::instructions::INSTRUCTIONS;
use rmcp::{
    handler::server::router::tool::ToolRouter,
    model::{InitializeRequestParams, InitializeResult, ServerCapabilities, ServerInfo},
    service::{RequestContext, RoleServer},
    tool_handler, ServerHandler,
};

#[derive(Clone)]
pub struct ScryerServer {
    tool_router: ToolRouter<Self>,
    /// Session-level active model. Set by `get_model` / `get_node`; used as the
    /// default if a tool call omits `project`.
    pub(crate) active_model: std::sync::Arc<std::sync::Mutex<Option<scryer_core::ModelRef>>>,
}

impl ScryerServer {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router_read()
                + Self::tool_router_nodes()
                + Self::tool_router_links()
                + Self::tool_router_misc(),
            active_model: std::sync::Arc::new(std::sync::Mutex::new(None)),
        }
    }
}

#[tool_handler]
impl ServerHandler for ScryerServer {
    fn get_info(&self) -> ServerInfo {
        let instructions = format!(
            "{}\n\n## Modeling Rules\n{}",
            INSTRUCTIONS,
            scryer_core::rules::RULES
        );
        ServerInfo {
            instructions: Some(instructions.into()),
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
