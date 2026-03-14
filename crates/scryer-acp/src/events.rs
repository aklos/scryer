use serde::Serialize;

/// Events emitted during an agent session, forwarded to the Tauri frontend.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AgentEvent {
    /// Streaming text from the agent's response.
    Message { text: String },
    /// Chain-of-thought / thinking output.
    Thought { text: String },
    /// A tool call was initiated or updated.
    ToolCall {
        id: String,
        name: String,
        status: String,
    },
    /// The agent produced an execution plan.
    Plan { content: String },
    /// The session completed normally.
    Completed { stop_reason: String },
    /// The session failed with an error.
    Failed { error: String },
    /// The session was cancelled.
    Cancelled,
}
