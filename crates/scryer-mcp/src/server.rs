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
