//! Progress notes on planned claims a session leaves unbuilt — what is built,
//! what is left, what it waits on — read by the user where the claim lives.

use crate::helpers::*;
use crate::server::ScryerServer;
use crate::types::*;
use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, Content},
    tool, tool_router, ErrorData as McpError,
};

fn err(msg: impl Into<String>) -> CallToolResult {
    CallToolResult::error(vec![Content::text(msg.into())])
}

#[tool_router(router = tool_router_notes, vis = "pub(crate)")]
impl ScryerServer {
    #[tool(
        description = "Progress note on planned claims you leave unfolded: `notes` maps claim id → \
         built / left / waits on (empty clears). The user sees it on the claim.\n\
         Rules: loop-sign-off"
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
}
