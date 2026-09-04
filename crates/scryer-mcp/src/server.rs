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
                + Self::tool_router_intent()
                + Self::tool_router_testing(),
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
            .tool_router
            .list_all()
            .into_iter()
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
        assert!(total <= 15_000, "descriptions total {total} chars (budget 15000)");
        for (name, d) in &descs {
            assert!(d.len() <= 800, "{name} description is {} chars (max 800)", d.len());
        }
    }
}
