//! The ask ledger over MCP. The UserPromptSubmit hook logs each prompt; the
//! agent breaks it into asks here and resolves each one — delivered by
//! verified claims, answered, or descoped with a reason. The Stop hook holds
//! the agent to it.

use crate::helpers::*;
use crate::server::ScryerServer;
use crate::types::*;
use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, Content},
    tool, tool_router, ErrorData as McpError,
};
use scryer_core::session::{AskKind, AskStatus, AskView, NewAsk, Resolution};
use scryer_core::ModelRef;

fn err(msg: impl Into<String>) -> CallToolResult {
    CallToolResult::error(vec![Content::text(msg.into())])
}

const NO_SESSION_MSG: &str = "No agent session is known here, so there is no prompt to break into \
     asks. The ask ledger needs scryer's session hooks installed for this project.";

/// One line per ask: id, status, text, and for an open one what is missing.
fn ask_lines(views: &[AskView]) -> String {
    let mut out = String::new();
    for v in views {
        let status = match &v.status {
            AskStatus::Delivered => "delivered".to_string(),
            AskStatus::Answered => "answered".to_string(),
            AskStatus::Descoped { reason } => format!("descoped: {reason}"),
            AskStatus::Open { .. } => "open".to_string(),
        };
        out.push_str(&format!("{} [{status}] {}", v.id, v.text));
        if !v.claims.is_empty() {
            out.push_str(&format!(" ← {}", v.claims.join(", ")));
        }
        out.push('\n');
        if let AskStatus::Open { missing } = &v.status {
            for m in missing {
                out.push_str(&format!("    {m}\n"));
            }
        }
    }
    out
}

fn verified(model_ref: &ModelRef, claims: &[String]) -> std::collections::HashMap<String, bool> {
    scryer_extract::test_status::claim_evidence(model_ref, claims)
        .map(|m| m.into_iter().map(|(k, e)| (k, e.verified())).collect())
        .unwrap_or_default()
}

#[tool_router(router = tool_router_asks, vis = "pub(crate)")]
impl ScryerServer {
    #[tool(
        description = "Break a logged user prompt into asks — one per distinct thing it asks for, in \
         the user's terms. `kind: \"answer\"` for a question; default `build`. A port / \
         match-the-reference prompt needs one build ask per feature of the source, each with \
         `source`. An empty `asks` says the prompt asked for nothing new.\n\
         Rules: ask-ledger"
    )]
    pub(crate) fn file_asks(
        &self,
        Parameters(req): Parameters<FileAsksRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model_ref = resolve_model_ref(req.project.as_deref())?;
        let Some(session) = self.session_id(&model_ref) else {
            return Ok(err(NO_SESSION_MSG));
        };
        let mut asks = Vec::new();
        for a in req.asks {
            let kind = match a.kind.as_deref().map(str::trim) {
                None | Some("") | Some("build") => AskKind::Build,
                Some("answer") => AskKind::Answer,
                Some(other) => {
                    return Ok(err(format!("unknown ask kind '{other}' — \"build\" or \"answer\"")))
                }
            };
            asks.push(NewAsk { text: a.text, kind, source: a.source });
        }
        match scryer_core::session::file_asks(&model_ref, &session, req.prompt.as_deref(), asks) {
            Ok((prompt, filed)) if filed.is_empty() => Ok(CallToolResult::success(vec![Content::text(
                format!("{prompt}: nothing new asked."),
            )])),
            Ok((prompt, filed)) => {
                let mut msg = format!("{prompt} → ");
                msg.push_str(
                    &filed.iter().map(|a| format!("{} \"{}\"", a.id, a.text)).collect::<Vec<_>>().join(", "),
                );
                msg.push_str(
                    "\nDeliver each: a build ask by claims you model, implement, test and link \
                     (resolve_ask {id, claims}); an answer ask by answering it \
                     (resolve_ask {id, answered: true}).",
                );
                Ok(CallToolResult::success(vec![Content::text(msg)]))
            }
            Err(e) => Ok(err(e)),
        }
    }

    #[tool(
        description = "Resolve an ask: `claims` links the claims that deliver a build ask (delivered \
         once each has a passing verdict and this session edited its anchored code); \
         `answered: true` closes an answer ask; `descoped` drops an ask with a one-line reason \
         the user reads. Returns where every ask stands.\n\
         Rules: ask-ledger"
    )]
    pub(crate) fn resolve_ask(
        &self,
        Parameters(req): Parameters<ResolveAskRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model_ref = resolve_model_ref(req.project.as_deref())?;
        let Some(session) = self.session_id(&model_ref) else {
            return Ok(err(NO_SESSION_MSG));
        };
        let how = match (req.claims, req.answered, req.descoped) {
            (Some(c), None | Some(false), None) if !c.is_empty() => Resolution::Claims(c),
            (None, Some(true), None) => Resolution::Answered,
            (None, None | Some(false), Some(r)) => Resolution::Descoped(r),
            _ => return Ok(err("Pass exactly one of `claims`, `answered: true` or `descoped`.")),
        };
        if let Err(e) = scryer_core::session::resolve_ask(&model_ref, &session, &req.id, how) {
            return Ok(err(e));
        }
        let view = scryer_core::session::session_view(&model_ref, &session, |c| verified(&model_ref, c));
        Ok(CallToolResult::success(vec![Content::text(ask_lines(&view.asks))]))
    }

    #[tool(
        description = "Progress note on planned claims you leave unfolded: `notes` maps claim id → \
         built / left / waits on (empty clears). The user sees it on the claim.\n\
         Rules: ask-ledger"
    )]
    pub(crate) fn note_claims(
        &self,
        Parameters(req): Parameters<NoteClaimsRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model_ref = resolve_model_ref(req.project.as_deref())?;
        let _lock = match lock_or_err(&model_ref) {
            Ok(l) => l,
            Err(e) => return Ok(e),
        };
        match scryer_core::note_claims(&model_ref, &req.notes) {
            Ok(refused) if refused.len() == req.notes.len() && !refused.is_empty() => Ok(err(format!(
                "None of these is a pending planned claim: {}. Folded claims need no note.",
                refused.join(", ")
            ))),
            Ok(refused) => {
                let mut msg = format!("Noted {} claim(s).", req.notes.len() - refused.len());
                if !refused.is_empty() {
                    msg.push_str(&format!(" Not pending, skipped: {}.", refused.join(", ")));
                }
                Ok(CallToolResult::success(vec![Content::text(msg)]))
            }
            Err(e) => Ok(err(e)),
        }
    }

    #[tool(
        description = "This session's asks and where each stands (with what is still missing for \
         open ones), plus edited files no ask accounts for.\n\
         Rules: ask-ledger"
    )]
    pub(crate) fn get_asks(
        &self,
        Parameters(req): Parameters<GetAsksRequest>,
    ) -> Result<CallToolResult, McpError> {
        let model_ref = resolve_model_ref(req.project.as_deref())?;
        let Some(session) = self.session_id(&model_ref) else {
            return Ok(err(NO_SESSION_MSG));
        };
        let view = scryer_core::session::session_view(&model_ref, &session, |c| verified(&model_ref, c));
        let mut msg = ask_lines(&view.asks);
        if !view.unfiled.is_empty() {
            msg.push_str(&format!("Prompts not broken into asks yet: {}\n", view.unfiled.join(", ")));
        }
        if !view.untraced.is_empty() {
            msg.push_str(&format!("Edits no ask accounts for: {}\n", view.untraced.join(", ")));
        }
        if msg.is_empty() {
            msg = "No prompts logged in this session yet.".into();
        }
        Ok(CallToolResult::success(vec![Content::text(msg)]))
    }
}
