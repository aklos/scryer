use agent_client_protocol::{
    self as acp, Client, ContentBlock, RequestPermissionRequest, RequestPermissionResponse,
    SelectedPermissionOutcome, SessionNotification, SessionUpdate,
};
use tokio::sync::mpsc;

use crate::events::AgentEvent;

/// Tools that should be rejected — get_task would start an implementation loop.
const REJECT_TOOLS: &[&str] = &["get_task"];

fn should_reject(title: &str) -> bool {
    let name = title.rsplit("__").next().unwrap_or(title);
    REJECT_TOOLS.iter().any(|t| *t == name)
}

/// ACP Client implementation that auto-approves all tool calls
/// (except get_task) and forwards session notifications as `AgentEvent`s.
pub struct ScryerClient {
    event_tx: mpsc::UnboundedSender<AgentEvent>,
}

impl ScryerClient {
    pub fn new(event_tx: mpsc::UnboundedSender<AgentEvent>) -> Self {
        Self { event_tx }
    }

    fn send(&self, event: AgentEvent) {
        let _ = self.event_tx.send(event);
    }
}

#[async_trait::async_trait(?Send)]
impl Client for ScryerClient {
    async fn request_permission(
        &self,
        args: RequestPermissionRequest,
    ) -> acp::Result<RequestPermissionResponse> {
        let tool_title = args.tool_call.fields.title.as_deref().unwrap_or("");

        let option = if should_reject(tool_title) {
            // Reject get_task — agent shouldn't start an implementation loop
            args.options
                .iter()
                .find(|o| matches!(o.kind, acp::PermissionOptionKind::RejectOnce))
                .or(args.options.last())
        } else {
            // Auto-approve everything else
            args.options
                .iter()
                .find(|o| matches!(o.kind, acp::PermissionOptionKind::AllowAlways))
                .or_else(|| {
                    args.options
                        .iter()
                        .find(|o| matches!(o.kind, acp::PermissionOptionKind::AllowOnce))
                })
                .or(args.options.first())
        };

        let outcome = match option {
            Some(opt) => acp::RequestPermissionOutcome::Selected(
                SelectedPermissionOutcome::new(opt.option_id.clone()),
            ),
            None => acp::RequestPermissionOutcome::Cancelled,
        };
        Ok(RequestPermissionResponse::new(outcome))
    }

    async fn session_notification(
        &self,
        args: SessionNotification,
    ) -> acp::Result<()> {
        match args.update {
            SessionUpdate::AgentMessageChunk(chunk) => {
                if let Some(text) = extract_text(&chunk.content) {
                    self.send(AgentEvent::Message { text });
                }
            }
            SessionUpdate::AgentThoughtChunk(chunk) => {
                if let Some(text) = extract_text(&chunk.content) {
                    self.send(AgentEvent::Thought { text });
                }
            }
            SessionUpdate::ToolCall(tc) => {
                self.send(AgentEvent::ToolCall {
                    id: tc.tool_call_id.to_string(),
                    name: tc.title,
                    status: format!("{:?}", tc.status),
                });
            }
            SessionUpdate::ToolCallUpdate(tcu) => {
                self.send(AgentEvent::ToolCall {
                    id: tcu.tool_call_id.to_string(),
                    name: tcu.fields.title.unwrap_or_default(),
                    status: tcu
                        .fields
                        .status
                        .map(|s| format!("{:?}", s))
                        .unwrap_or_default(),
                });
            }
            SessionUpdate::Plan(plan) => {
                let content = plan
                    .entries
                    .iter()
                    .map(|entry| entry.content.clone())
                    .collect::<Vec<_>>()
                    .join("\n");
                self.send(AgentEvent::Plan { content });
            }
            _ => {}
        }
        Ok(())
    }
}

fn extract_text(block: &ContentBlock) -> Option<String> {
    match block {
        ContentBlock::Text(t) => Some(t.text.clone()),
        _ => None,
    }
}
