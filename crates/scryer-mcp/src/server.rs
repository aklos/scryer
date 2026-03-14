use crate::instructions::INSTRUCTIONS;
use rmcp::{
    handler::server::router::tool::ToolRouter,
    model::{ServerCapabilities, ServerInfo},
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
                + Self::tool_router_edges()
                + Self::tool_router_task()
                + Self::tool_router_misc(),
        }
    }
}

#[tool_handler]
impl ServerHandler for ScryerServer {
    fn get_info(&self) -> ServerInfo {
        let instructions = format!(
            "{}\n\n## C4 Modeling Rules\n{}",
            INSTRUCTIONS,
            scryer_core::rules::RULES
        );
        ServerInfo {
            instructions: Some(instructions.into()),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}
