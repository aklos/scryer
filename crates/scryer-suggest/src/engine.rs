use llm::builder::{LLMBackend, LLMBuilder};
use llm::chat::ChatMessage;

use scryer_core::AiSettings;

fn map_backend(provider: &str) -> Result<LLMBackend, String> {
    match provider {
        "openai" => Ok(LLMBackend::OpenAI),
        "anthropic" => Ok(LLMBackend::Anthropic),
        "google" => Ok(LLMBackend::Google),
        "ollama" => Ok(LLMBackend::Ollama),
        "groq" => Ok(LLMBackend::Groq),
        "mistral" => Ok(LLMBackend::Mistral),
        "deepseek" => Ok(LLMBackend::DeepSeek),
        other => Err(format!("unknown provider: {other}")),
    }
}

pub async fn generate(
    settings: &AiSettings,
    system: &str,
    user_msg: &str,
) -> Result<String, String> {
    let backend = map_backend(&settings.provider)?;

    let mut builder = LLMBuilder::new()
        .backend(backend)
        .model(&settings.model)
        .system(system);

    if !settings.api_key.is_empty() {
        builder = builder.api_key(&settings.api_key);
    }

    let llm = builder.build().map_err(|e| format!("build LLM: {e}"))?;

    let messages = vec![ChatMessage::user().content(user_msg).build()];

    let response = llm.chat(&messages).await.map_err(|e| format!("chat: {e}"))?;

    match response.text() {
        Some(text) if !text.trim().is_empty() => Ok(text),
        Some(_) => Err("LLM returned empty text".to_string()),
        None => Err("LLM returned no text".to_string()),
    }
}
