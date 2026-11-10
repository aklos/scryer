//! The MCP server: one stdio process per agent session, advertising the read
//! and testing tools — their input schemas stripped of generator noise once at
//! construction — and the connect-time instructions. The session it serves is
//! the one its harness process last bound, else the one named in the env.
//!
//! `show` is listed only while the desktop app has the working project open,
//! and the client hears `tools/list_changed` when that flips: the server
//! checks every couple of seconds once the client is initialized. Polling,
//! because the alternative, a file watcher per agent process, costs more than
//! the one `stat` a check is.

use crate::instructions::instructions_for;
use rmcp::{
    handler::server::{router::tool::ToolRouter, tool::ToolCallContext},
    model::{
        CallToolRequestParams, CallToolResult, Implementation, InitializeRequestParams,
        InitializeResult, ListToolsResult, PaginatedRequestParams, ServerCapabilities, ServerInfo, Tool,
    },
    service::{NotificationContext, RequestContext, RoleServer},
    ErrorData as McpError, ServerHandler,
};
use scryer_core::agent_integration::Tool as AgentTool;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct ScryerServer {
    tool_router: ToolRouter<Self>,
    /// The tool list as advertised: the router's tools with their input
    /// schemas slimmed once at construction (see [`slim_schema`]).
    tools: Vec<Tool>,
    /// A session id pinned in-process (tests); otherwise the harness's own
    /// (see [`ScryerServer::session_id`]).
    session_override: Arc<Mutex<Option<String>>>,
    /// The agent CLIs on PATH, read once: whose skill and hooks a missing
    /// setup is nudged for. None in tests, unless a test names them.
    detected: Vec<AgentTool>,
}

impl ScryerServer {
    pub fn new() -> Self {
        let tool_router =
            Self::tool_router_read() + Self::tool_router_testing() + Self::tool_router_comments() + Self::tool_router_show();
        let tools = tool_router
            .list_all()
            .into_iter()
            .map(|mut t| {
                let mut schema = serde_json::Value::Object((*t.input_schema).clone());
                slim_schema(&mut schema);
                if let serde_json::Value::Object(o) = schema {
                    t.input_schema = Arc::new(o);
                }
                t
            })
            .collect();
        Self {
            tool_router,
            tools,
            session_override: Arc::new(Mutex::new(None)),
            detected: if cfg!(test) { Vec::new() } else { scryer_core::agent_setup::detect_tools() },
        }
    }

    /// A server that takes `detected` for the agent CLIs on PATH.
    #[cfg(test)]
    pub(crate) fn with_detected(detected: &[AgentTool]) -> Self {
        Self { detected: detected.to_vec(), ..Self::new() }
    }

    /// The agent CLIs this server nudges setup for (see [`Self::detected`]).
    pub(crate) fn detected(&self) -> &[AgentTool] {
        &self.detected
    }

    /// Every tool this server has (slimmed schemas), `show` included.
    #[cfg(test)]
    pub(crate) fn tools(&self) -> &[Tool] {
        &self.tools
    }

    /// The tools `tools/list` returns: all of them while the app is open, else all but `show`.
    pub(crate) fn listed_tools(&self, app_open: bool) -> Vec<Tool> {
        self.tools.iter().filter(|t| app_open || t.name != "show").cloned().collect()
    }

    /// What the app calls this session when it shows something for it: the session's task, else "an agent".
    pub(crate) fn sender(&self, model_ref: &scryer_core::ModelRef) -> String {
        self.session_id(model_ref)
            .and_then(|s| scryer_core::session_hooks::session_task(model_ref, &s))
            .unwrap_or_else(|| "an agent".into())
    }

    /// A markdown link that opens the app at node `id`, named `name`.
    pub(crate) fn node_link(&self, model_ref: &scryer_core::ModelRef, id: &str, name: &str) -> String {
        let d = scryer_core::app_view::Direction { sender: self.sender(model_ref), node: id.into(), ..Default::default() };
        format!("[{name}]({})", scryer_core::app_view::link_for(model_ref.project_path(), &d))
    }

    /// Whether the desktop app has the working project open.
    pub(crate) fn app_is_open() -> bool {
        std::env::current_dir()
            .ok()
            .map(crate::cli::working_project)
            .is_some_and(|p| scryer_core::app_view::app_open(&p).is_some())
    }

    /// A server pinned to session `id` — how a test stands in for the
    /// harness's session.
    #[cfg(test)]
    pub(crate) fn for_session(id: &str) -> Self {
        let server = Self::new();
        server.set_session(id);
        server
    }

    #[cfg(test)]
    pub(crate) fn set_session(&self, id: &str) {
        if let Ok(mut s) = self.session_override.lock() {
            *s = Some(id.to_string());
        }
    }

    /// The agent session this server serves. The server is stdio, one process
    /// per agent session: the session its harness process last bound (the
    /// hooks keep that current across `/clear`), else the one the harness
    /// named in the env at spawn.
    pub(crate) fn session_id(&self, model_ref: &scryer_core::ModelRef) -> Option<String> {
        if let Some(id) = self.session_override.lock().ok().and_then(|s| s.clone()) {
            return Some(id);
        }
        #[cfg(unix)]
        if let Some(id) =
            scryer_core::session::session_for_pid(model_ref, std::os::unix::process::parent_id())
        {
            return Some(id);
        }
        #[cfg(not(unix))]
        let _ = model_ref;
        std::env::var("CLAUDE_CODE_SESSION_ID").ok().filter(|s| !s.is_empty())
    }
}

/// Strip the keys a JSON-schema generator emits that carry nothing for a
/// model reading the tool list: the `$schema` dialect URL, `default: null`,
/// `nullable`, `format`, `minimum`, and per-type `title`s. Optionality is already expressed by
/// `required`; the rest is validator metadata. `$defs`/`$ref` are left alone.
/// Runs once per tool at construction, so the generator's output shape can
/// change under a schemars upgrade without touching the request types.
pub(crate) fn slim_schema(v: &mut serde_json::Value) {
    match v {
        serde_json::Value::Object(map) => {
            map.retain(|k, val| {
                !(k == "nullable"
                    || k == "$schema"
                    || k == "format"
                    || k == "minimum"
                    || k == "title"
                    || (k == "default" && val.is_null()))
            });
            for val in map.values_mut() {
                slim_schema(val);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                slim_schema(item);
            }
        }
        _ => {}
    }
}

impl ServerHandler for ScryerServer {
    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let tcc = ToolCallContext::new(self, request, context);
        self.tool_router.call(tcc).await
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        Ok(ListToolsResult {
            tools: self.listed_tools(Self::app_is_open()),
            meta: None,
            next_cursor: None,
        })
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        self.tools.iter().find(|t| t.name == name).cloned()
    }

    fn get_info(&self) -> ServerInfo {
        // Always-loaded context: one paragraph, the loop and the claim grammar,
        // and the setup nudge when the working project needs one.
        let project = std::env::current_dir().ok().map(crate::cli::working_project);
        ServerInfo {
            server_info: Implementation {
                name: "scryer".into(),
                version: env!("CARGO_PKG_VERSION").into(),
                ..Default::default()
            },
            instructions: Some(instructions_for(project.as_deref(), &self.detected)),
            capabilities: ServerCapabilities::builder().enable_tools().enable_tool_list_changed().build(),
            ..Default::default()
        }
    }

    fn on_initialized(
        &self,
        context: NotificationContext<RoleServer>,
    ) -> impl std::future::Future<Output = ()> + Send + '_ {
        let peer = context.peer;
        tokio::spawn(async move {
            let mut open = Self::app_is_open();
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                let now = Self::app_is_open();
                if now != open {
                    open = now;
                    if peer.notify_tool_list_changed().await.is_err() {
                        break;
                    }
                }
            }
        });
        std::future::ready(())
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

#[cfg(test)]
mod surface {
    //! The always-loaded surface — instructions, descriptions, schemas — stays
    //! small, and serves no rule bodies: what an agent needs is inlined where
    //! it acts.
    use super::*;
    use crate::instructions::INSTRUCTIONS;

    fn descriptions() -> Vec<(String, String)> {
        ScryerServer::new()
            .tools()
            .iter()
            .cloned()
            .map(|t| (t.name.to_string(), t.description.map(|d| d.to_string()).unwrap_or_default()))
            .collect()
    }

    /// The daily set and nothing else: reads of the derived tree, the test
    /// loop, the comment threads, and pointing the app at a node.
    #[test]
    fn the_surface_is_the_daily_set() {
        let mut names: Vec<String> = descriptions().into_iter().map(|(n, _)| n).collect();
        names.sort();
        assert_eq!(
            names,
            [
                "close_probe",
                "comment",
                "comments",
                "get_test_radius",
                "ingest_mutation_report",
                "ingest_test_report",
                "locate",
                "open_probe",
                "orient",
                "read_model",
                "search_model",
                "show",
            ]
        );
    }

    /// While the app has the project open, lists the show tool, and otherwise leaves it out.
    #[test]
    fn lists_show_only_while_the_app_is_open() {
        let server = ScryerServer::new();
        let names = |open| server.listed_tools(open).into_iter().map(|t| t.name.to_string()).collect::<Vec<_>>();
        assert!(names(true).contains(&"show".to_string()));
        assert!(!names(false).contains(&"show".to_string()));
        assert_eq!(names(false).len() + 1, names(true).len());
    }

    /// Nothing points the agent at a rule to fetch: no `Rules:` lines, no
    /// `[[slug]]` citations, no tool that serves them.
    #[test]
    fn nothing_on_the_surface_cites_a_rule() {
        assert!(!INSTRUCTIONS.contains("[["), "instructions cite a rule");
        assert!(!INSTRUCTIONS.contains("get_rules"));
        for (name, desc) in descriptions() {
            assert!(!desc.contains("Rules:") && !desc.contains("[["), "{name} cites a rule");
        }
    }

    /// The instructions carry what an agent needs before writing a stub: the
    /// EARS patterns and which test level carries which claim.
    #[test]
    fn the_instructions_carry_the_claim_grammar_and_the_levels() {
        for must in ["When <", "While <", "If <", "then <", "e2e", "integration", "unit", "todo test", "get_test_radius"] {
            assert!(INSTRUCTIONS.contains(must), "instructions lack {must:?}");
        }
    }

    /// The always-loaded prose. Grows only by deliberate choice: raise the
    /// numbers here in the same change that adds the text.
    #[test]
    fn instructions_and_descriptions_stay_within_budget() {
        assert!(INSTRUCTIONS.len() <= 1_800, "instructions are {} chars (budget 1800)", INSTRUCTIONS.len());
        let descs = descriptions();
        let total: usize = descs.iter().map(|(_, d)| d.len()).sum();
        assert!(total <= 4_500, "descriptions total {total} chars (budget 4500)");
        for (name, d) in &descs {
            assert!(d.len() <= 600, "{name} description is {} chars (max 600)", d.len());
        }
    }

    #[test]
    fn slim_schema_drops_generator_noise_and_keeps_structure() {
        let mut v = serde_json::json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "title": "Req",
            "properties": {
                "n": {"type": "integer", "format": "uint32", "minimum": 0, "nullable": true},
                "s": {"type": "string", "default": null, "description": "kept"},
                "d": {"type": "string", "default": ""},
                "items": {"type": "array", "items": {"$ref": "#/$defs/X", "nullable": true}}
            },
            "required": ["n"],
            "$defs": {"X": {"type": "object", "title": "X"}}
        });
        slim_schema(&mut v);
        assert_eq!(
            v,
            serde_json::json!({
                "type": "object",
                "properties": {
                    "n": {"type": "integer"},
                    "s": {"type": "string", "description": "kept"},
                    "d": {"type": "string", "default": ""},
                    "items": {"type": "array", "items": {"$ref": "#/$defs/X"}}
                },
                "required": ["n"],
                "$defs": {"X": {"type": "object"}}
            })
        );
    }

    /// The schemas are the bulk of the always-loaded surface; budgets are on
    /// the advertised (slimmed) list, as a client sees it.
    #[test]
    fn tool_schemas_stay_within_budget() {
        let mut total = 0;
        for t in ScryerServer::new().tools() {
            let text = serde_json::to_string(&serde_json::Value::Object((*t.input_schema).clone())).unwrap();
            total += text.len();
            for noise in ["\"$schema\"", "\"nullable\"", "\"format\"", "\"minimum\"", "\"title\"", "\"default\":null"] {
                assert!(!text.contains(noise), "{}: schema still carries {noise}", t.name);
            }
        }
        assert!(total <= 5_000, "schemas total {total} chars (budget 5000)");
    }

    /// Names itself scryer, with its version, to the client.
    #[test]
    fn the_server_names_itself() {
        let info = ScryerServer::new().get_info().server_info;
        assert_eq!((info.name.as_str(), info.version.as_str()), ("scryer", env!("CARGO_PKG_VERSION")));
    }

    /// If the working project lacks scryer's skill or hooks for a detected tool, then the connect-time instructions end with the setup nudge.
    #[test]
    fn the_instructions_carry_the_setup_nudge() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        assert_eq!(crate::cli::working_project(p.to_path_buf()), p, "no .scryer/ yet: the directory itself");
        assert_eq!(instructions_for(Some(p), &[]), INSTRUCTIONS);
        assert_eq!(instructions_for(None, &[AgentTool::ClaudeCode]), INSTRUCTIONS);
        let text = instructions_for(Some(p), &[AgentTool::ClaudeCode]);
        assert!(text.starts_with(INSTRUCTIONS), "{text}");
        assert!(text.ends_with("ends this note."), "{text}");
        assert!(text.contains("Claude Code lacks scryer's") && text.contains("`scryer-mcp setup on`"), "{text}");
    }
}
