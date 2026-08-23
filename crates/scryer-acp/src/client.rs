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

#[cfg(test)]
mod tests {
    use super::*;

    fn client() -> (ScryerClient, mpsc::UnboundedReceiver<AgentEvent>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (ScryerClient::new(tx), rx)
    }

    fn permission_request(title: &str, kinds: &[(&str, &str)]) -> RequestPermissionRequest {
        let options: Vec<serde_json::Value> = kinds
            .iter()
            .map(|(id, kind)| serde_json::json!({ "optionId": id, "name": id, "kind": kind }))
            .collect();
        serde_json::from_value(serde_json::json!({
            "sessionId": "s1",
            "toolCall": { "toolCallId": "tc1", "title": title },
            "options": options,
        }))
        .unwrap()
    }

    fn selected_option(resp: &RequestPermissionResponse) -> String {
        let v = serde_json::to_value(resp).unwrap();
        assert_eq!(v["outcome"]["outcome"], "selected", "an option must be selected: {v}");
        v["outcome"]["optionId"].as_str().unwrap().to_string()
    }

    /// get_task would start an implementation loop — its permission request is
    /// answered with the reject option, MCP-prefixed or not.
    #[tokio::test]
    async fn a_get_task_permission_request_is_rejected() {
        let (client, _rx) = client();
        let opts =
            [("allow-always", "allow_always"), ("allow-once", "allow_once"), ("reject", "reject_once")];
        for title in ["get_task", "mcp__scryer__get_task"] {
            let resp = client.request_permission(permission_request(title, &opts)).await.unwrap();
            assert_eq!(selected_option(&resp), "reject", "{title} must be rejected");
        }
    }

    /// Every other tool is auto-approved — allow-always preferred, allow-once
    /// when that's all the agent offers.
    #[tokio::test]
    async fn any_other_tool_is_auto_approved() {
        let (client, _rx) = client();
        let resp = client
            .request_permission(permission_request(
                "mcp__scryer__set_node",
                &[("reject", "reject_once"), ("allow-once", "allow_once"), ("allow-always", "allow_always")],
            ))
            .await
            .unwrap();
        assert_eq!(selected_option(&resp), "allow-always");

        let resp = client
            .request_permission(permission_request(
                "Bash",
                &[("reject", "reject_once"), ("allow-once", "allow_once")],
            ))
            .await
            .unwrap();
        assert_eq!(selected_option(&resp), "allow-once");
    }

    /// Session notifications come out the other side as their corresponding
    /// AgentEvent: message, thought, tool call, plan.
    #[tokio::test]
    async fn session_notifications_forward_as_agent_events() {
        let (client, mut rx) = client();
        let notify = |update: serde_json::Value| -> SessionNotification {
            serde_json::from_value(serde_json::json!({ "sessionId": "s1", "update": update }))
                .unwrap()
        };

        client
            .session_notification(notify(serde_json::json!({
                "sessionUpdate": "agent_message_chunk",
                "content": { "type": "text", "text": "hello" },
            })))
            .await
            .unwrap();
        assert!(matches!(rx.try_recv(), Ok(AgentEvent::Message { text }) if text == "hello"));

        client
            .session_notification(notify(serde_json::json!({
                "sessionUpdate": "agent_thought_chunk",
                "content": { "type": "text", "text": "hmm" },
            })))
            .await
            .unwrap();
        assert!(matches!(rx.try_recv(), Ok(AgentEvent::Thought { text }) if text == "hmm"));

        client
            .session_notification(notify(serde_json::json!({
                "sessionUpdate": "tool_call",
                "toolCallId": "t1",
                "title": "Read",
            })))
            .await
            .unwrap();
        assert!(matches!(
            rx.try_recv(),
            Ok(AgentEvent::ToolCall { id, name, .. }) if id == "t1" && name == "Read"
        ));

        client
            .session_notification(notify(serde_json::json!({
                "sessionUpdate": "plan",
                "entries": [
                    { "content": "step one", "priority": "medium", "status": "pending" },
                    { "content": "step two", "priority": "medium", "status": "pending" },
                ],
            })))
            .await
            .unwrap();
        assert!(matches!(
            rx.try_recv(),
            Ok(AgentEvent::Plan { content }) if content == "step one\nstep two"
        ));
    }
}
