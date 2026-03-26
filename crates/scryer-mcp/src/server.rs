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
    /// Session-level active model. Set by `get_model`/explicit name, used as
    /// default when tools omit the model parameter.
    pub(crate) active_model: std::sync::Arc<std::sync::Mutex<Option<scryer_core::ModelRef>>>,
}

impl ScryerServer {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router_read()
                + Self::tool_router_nodes()
                + Self::tool_router_edges()
                + Self::tool_router_task()
                + Self::tool_router_misc(),
            active_model: std::sync::Arc::new(std::sync::Mutex::new(None)),
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

    fn initialize(
        &self,
        request: InitializeRequestParams,
        context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<InitializeResult, rmcp::ErrorData>> + Send + '_ {
        // Record which client connected so the Tauri app can use ACP with the same agent
        let client_name = request.client_info.name.clone();
        let client_version = request.client_info.version.clone();
        write_active_client(&client_name, &client_version);

        // Default behavior: store peer info and return server info
        if context.peer.peer_info().is_none() {
            context.peer.set_peer_info(request);
        }
        std::future::ready(Ok(self.get_info()))
    }
}

/// Write the connected client identity to ~/.scryer/active-client.json
/// so the Tauri app knows which agent to launch via ACP.
fn write_active_client(name: &str, version: &str) {
    let dir = scryer_core::models_dir();
    let path = dir.join("active-client.json");
    let data = serde_json::json!({
        "name": name,
        "version": version,
    });
    if let Ok(json) = serde_json::to_string_pretty(&data) {
        let _ = std::fs::write(&path, json);
    }
}
