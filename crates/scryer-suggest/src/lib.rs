pub mod engine;
mod parse;
mod prompt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hint {
    pub node_id: String,
    pub message: String,
    pub severity: HintSeverity,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HintSeverity {
    Info,
    Warning,
}

/// Run LLM hint analysis on a diagram via API. Returns empty vec on failure.
pub async fn get_hints(
    model: &scryer_core::C4ModelData,
    settings: &scryer_core::AiSettings,
) -> Vec<Hint> {
    let system = prompt::system_prompt();
    let user_msg = prompt::user_message(model);

    eprintln!("[scryer-suggest] sending to {} ({})", settings.provider, settings.model);

    match engine::generate(settings, &system, &user_msg).await {
        Ok(raw) => {
            eprintln!("[scryer-suggest] raw LLM output:\n{}", raw);
            let hints = parse::parse_llm_output(&raw, model);
            eprintln!("[scryer-suggest] parsed {} hints", hints.len());
            hints
        }
        Err(e) => {
            eprintln!("[scryer-suggest] generate error: {}", e);
            vec![]
        }
    }
}
