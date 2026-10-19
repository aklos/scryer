//! Strips JSON-schema generator noise (dialect URLs, null defaults, titles) from advertised tool schemas so a client model isn't taxed reading them
//!
//! Reads and sets the session's current change id, scoped so a change opened in one project never tags writes into another
//!
//! Builds the server by combining every tool category's router and slimming each tool's advertised schema

use crate::instructions::INSTRUCTIONS;
use rmcp::{
    handler::server::{router::tool::ToolRouter, tool::ToolCallContext},
    model::{
        CallToolRequestParams, CallToolResult, InitializeRequestParams, InitializeResult,
        ListToolsResult, PaginatedRequestParams, ServerCapabilities, ServerInfo, Tool,
    },
    service::{RequestContext, RoleServer},
    ErrorData as McpError, ServerHandler,
};
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
}

impl ScryerServer {
    pub fn new() -> Self {
        let tool_router = Self::tool_router_read() + Self::tool_router_testing();
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
        }
    }

    /// The advertised tools (slimmed schemas) — what `tools/list` returns.
    #[cfg(test)]
    pub(crate) fn tools(&self) -> &[Tool] {
        &self.tools
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
            tools: self.tools.clone(),
            meta: None,
            next_cursor: None,
        })
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        self.tools.iter().find(|t| t.name == name).cloned()
    }

    fn get_info(&self) -> ServerInfo {
        // Always-loaded context: one paragraph, the loop and the claim grammar.
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

#[cfg(test)]
mod surface {
    //! The always-loaded surface — instructions, descriptions, schemas — stays
    //! small, and serves no rule bodies: what an agent needs is inlined where
    //! it acts.
    use super::*;

    fn descriptions() -> Vec<(String, String)> {
        ScryerServer::new()
            .tools()
            .iter()
            .cloned()
            .map(|t| (t.name.to_string(), t.description.map(|d| d.to_string()).unwrap_or_default()))
            .collect()
    }

    /// The daily set and nothing else: reads of the derived tree and the
    /// test loop.
    #[test]
    fn the_surface_is_the_daily_set() {
        let mut names: Vec<String> = descriptions().into_iter().map(|(n, _)| n).collect();
        names.sort();
        assert_eq!(
            names,
            [
                "close_probe",
                "get_test_radius",
                "ingest_mutation_report",
                "ingest_test_report",
                "locate",
                "open_probe",
                "orient",
                "read_model",
                "search_model",
            ]
        );
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
}
