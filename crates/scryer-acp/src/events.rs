use serde::Serialize;

/// Token usage (and cost) an agent reports at the end of a turn. Populated from
/// the CLI `result` event (Claude Code) or token-count event (Codex); summed
/// across the many sessions a build runs so the orchestrator can log a total.
#[derive(Debug, Clone, Copy, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_creation_input_tokens: u64,
    pub cache_read_input_tokens: u64,
    pub cost_usd: f64,
}

impl Usage {
    /// Every token billed this turn — fresh input, output, and both cache tiers.
    pub fn total_tokens(&self) -> u64 {
        self.input_tokens
            + self.output_tokens
            + self.cache_creation_input_tokens
            + self.cache_read_input_tokens
    }

    /// Fold another session's usage into this running total.
    pub fn add(&mut self, o: &Usage) {
        self.input_tokens += o.input_tokens;
        self.output_tokens += o.output_tokens;
        self.cache_creation_input_tokens += o.cache_creation_input_tokens;
        self.cache_read_input_tokens += o.cache_read_input_tokens;
        self.cost_usd += o.cost_usd;
    }
}

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
    /// End-of-turn token usage the agent reported. Intercepted by the build
    /// orchestrator (summed into the build total), ignored by the frontend.
    Usage { usage: Usage },
    /// Heartbeat: the agent is producing output (throttled).
    Activity,
    /// The session completed normally.
    Completed { stop_reason: String },
    /// The session failed with an error.
    Failed { error: String },
    /// The session was cancelled.
    Cancelled,
}
