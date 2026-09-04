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
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct ScryerServer {
    tool_router: ToolRouter<Self>,
    /// The tool list as advertised: the router's tools with their input
    /// schemas slimmed once at construction (see [`slim_schema`]).
    tools: Vec<Tool>,
    /// The change this SESSION is writing into (set via `open_change`), scoped
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
        let tool_router = Self::tool_router_read()
            + Self::tool_router_nodes()
            + Self::tool_router_links()
            + Self::tool_router_misc()
            + Self::tool_router_generation()
            + Self::tool_router_intent()
            + Self::tool_router_testing();
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
            current_change: Arc::new(Mutex::new(None)),
        }
    }

    /// The advertised tools (slimmed schemas) — what `tools/list` returns.
    #[cfg(test)]
    pub(crate) fn tools(&self) -> &[Tool] {
        &self.tools
    }

    /// A server whose session already has an open change on `project`, so a
    /// test can exercise a plan write without staging the ledger first.
    #[cfg(test)]
    pub(crate) fn with_change(project: &std::path::Path) -> Self {
        let server = Self::new();
        let model_ref = scryer_core::ModelRef::ProjectLocal(project.to_path_buf());
        let mut plan = scryer_core::read_planned_seeded_at(&model_ref).unwrap_or_default();
        let id = scryer_core::changes::open_change(&mut plan, "test fixture", 0);
        scryer_core::write_planned_at(&model_ref, &plan).unwrap();
        server.set_session_change(Some((project.to_path_buf(), id)));
        server
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

#[cfg(test)]
mod rule_wiring {
    //! The tool surface is a rule GRAPH: the instructions are the root, each
    //! tool description ends with a `Rules:` line, and rule bodies cite each
    //! other as [[slug]]. These tests keep the graph closed (every citation
    //! resolves), connected (every rule is reachable), and small (the
    //! always-loaded surface stays inside its token budget).
    use super::*;
    use scryer_core::rules;

    fn descriptions() -> Vec<(String, String)> {
        ScryerServer::new()
            .tools()
            .iter()
            .cloned()
            .map(|t| {
                (
                    t.name.to_string(),
                    t.description.map(|d| d.to_string()).unwrap_or_default(),
                )
            })
            .collect()
    }

    /// The slugs a description names on its trailing `Rules:` line.
    fn rules_line(desc: &str) -> Option<Vec<&str>> {
        let last = desc.lines().last()?;
        let rest = last.strip_prefix("Rules: ")?;
        Some(rest.split(',').map(str::trim).filter(|s| !s.is_empty()).collect())
    }

    fn cites_a_rule_number(text: &str) -> bool {
        let lower = text.to_lowercase();
        ["rule ", "rules "].iter().any(|k| {
            lower
                .match_indices(k)
                .any(|(i, _)| lower[i + k.len()..].starts_with(|c: char| c.is_ascii_digit()))
        })
    }

    #[test]
    fn every_description_ends_with_a_rules_line_that_resolves() {
        for (name, desc) in descriptions() {
            let slugs = rules_line(&desc)
                .unwrap_or_else(|| panic!("{name}: description has no trailing `Rules:` line"));
            assert!(!slugs.is_empty(), "{name}: empty Rules line");
            for s in slugs {
                assert!(rules::get(s).is_some(), "{name} cites unknown rule slug `{s}`");
            }
        }
        for s in rules::citations(INSTRUCTIONS) {
            assert!(rules::get(s).is_some(), "instructions cite unknown [[{s}]]");
        }
    }

    #[test]
    fn every_rule_is_reachable_from_the_surface() {
        let mut cited: std::collections::HashSet<&str> = std::collections::HashSet::new();
        let descs = descriptions();
        for (_, d) in &descs {
            for s in rules_line(d).unwrap_or_default() {
                cited.insert(rules::get(s).unwrap().slug);
            }
        }
        for s in rules::citations(INSTRUCTIONS) {
            cited.insert(rules::get(s).unwrap().slug);
        }
        for r in rules::RULES {
            for s in rules::citations(r.body) {
                cited.insert(rules::get(s).unwrap().slug);
            }
        }
        let orphans: Vec<&str> = rules::RULES
            .iter()
            .map(|r| r.slug)
            .filter(|s| !cited.contains(s))
            .collect();
        assert!(orphans.is_empty(), "rules nothing cites: {orphans:?}");
    }

    #[test]
    fn nothing_on_the_surface_cites_a_rule_by_number() {
        assert!(!cites_a_rule_number(INSTRUCTIONS), "instructions cite a rule number");
        for (name, desc) in descriptions() {
            assert!(!cites_a_rule_number(&desc), "{name} cites a rule number");
        }
        for r in rules::RULES {
            assert!(!cites_a_rule_number(r.body), "rule {} cites a rule number", r.slug);
        }
        assert!(cites_a_rule_number("see rule 22"));
        assert!(!cites_a_rule_number("the rule is"));
    }

    /// The always-loaded prose. Grows only by deliberate choice: raise the
    /// numbers here in the same change that adds the text.
    #[test]
    fn instructions_and_descriptions_stay_within_budget() {
        assert!(
            INSTRUCTIONS.len() <= 4_200,
            "instructions are {} chars (budget 4200, ~1k tokens)",
            INSTRUCTIONS.len()
        );
        let descs = descriptions();
        let total: usize = descs.iter().map(|(_, d)| d.len()).sum();
        assert!(total <= 16_000, "descriptions total {total} chars (budget 16000)");
        for (name, d) in &descs {
            assert!(d.len() <= 800, "{name} description is {} chars (max 800)", d.len());
        }
    }

    /// Every schema string a client sees, with the JSON path it sits at.
    fn schema_strings(v: &serde_json::Value, path: &str, out: &mut Vec<(String, String)>) {
        match v {
            serde_json::Value::Object(map) => {
                for (k, val) in map {
                    let p = format!("{path}/{k}");
                    if k == "description" {
                        if let Some(s) = val.as_str() {
                            out.push((p.clone(), s.to_string()));
                        }
                    }
                    schema_strings(val, &p, out);
                }
            }
            serde_json::Value::Array(items) => {
                for (i, item) in items.iter().enumerate() {
                    schema_strings(item, &format!("{path}/{i}"), out);
                }
            }
            _ => {}
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

    /// The schemas are the bulk of the always-loaded surface. Budgets are
    /// on the advertised (slimmed) list, as a client sees it.
    #[test]
    fn tool_schemas_stay_within_budget() {
        let server = ScryerServer::new();
        let mut total = 0;
        for t in server.tools() {
            let schema = serde_json::Value::Object((*t.input_schema).clone());
            let text = serde_json::to_string(&schema).unwrap();
            let desc = t.description.as_deref().unwrap_or("").len();
            total += text.len();
            assert!(
                text.len() + desc <= 4_500,
                "{}: {} schema + {desc} description chars (max 4500)",
                t.name,
                text.len()
            );
            for noise in ["\"$schema\"", "\"nullable\"", "\"format\"", "\"minimum\"", "\"title\"", "\"default\":null"] {
                assert!(!text.contains(noise), "{}: schema still carries {noise}", t.name);
            }
            let mut strings = Vec::new();
            schema_strings(&schema, "", &mut strings);
            for (path, s) in strings {
                assert!(
                    s.len() <= 160,
                    "{}: schema description at {path} is {} chars (max 160): {s}",
                    t.name,
                    s.len()
                );
            }
        }
        assert!(total <= 30_000, "schemas total {total} chars (budget 30000)");
    }
}
