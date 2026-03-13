use reqwest::Client;
use serde::Deserialize;

/// Fetch available chat/text model IDs from a provider's API.
/// Returns a sorted list filtered to models suitable for text generation.
pub async fn fetch_models(provider: &str, api_key: &str) -> Result<Vec<String>, String> {
    let client = Client::new();

    match provider {
        "ollama" => fetch_ollama(&client).await,
        "openai" => fetch_openai(&client, api_key).await,
        "groq" => fetch_openai_compat(&client, "https://api.groq.com/openai/v1/models", api_key).await,
        "deepseek" => fetch_openai_compat(&client, "https://api.deepseek.com/models", api_key).await,
        "mistral" => fetch_openai_compat(&client, "https://api.mistral.ai/v1/models", api_key).await,
        "anthropic" => fetch_anthropic(&client, api_key).await,
        "google" => fetch_google(&client, api_key).await,
        other => Err(format!("unknown provider: {other}")),
    }
}

// ── OpenAI ──

#[derive(Deserialize)]
struct OaiModelList {
    data: Vec<OaiModel>,
}
#[derive(Deserialize)]
struct OaiModel {
    id: String,
}

/// Prefixes for non-chat OpenAI models (image, audio, embedding, moderation, etc.)
const OPENAI_EXCLUDE: &[&str] = &[
    "dall-e", "tts-", "whisper", "text-embedding", "text-moderation",
    "davinci", "babbage", "curie", "ada", "canary-",
    "omni-moderation", "codex-",
];

fn is_openai_chat_model(id: &str) -> bool {
    !OPENAI_EXCLUDE.iter().any(|prefix| id.starts_with(prefix))
}

async fn fetch_openai(client: &Client, api_key: &str) -> Result<Vec<String>, String> {
    let list = fetch_raw_oai(client, "https://api.openai.com/v1/models", api_key).await?;
    let mut ids: Vec<String> = list
        .into_iter()
        .filter(|id| is_openai_chat_model(id))
        .collect();
    ids.sort();
    Ok(ids)
}

// Generic OpenAI-compatible endpoint (Groq, DeepSeek, Mistral) — no filtering needed,
// these providers only expose chat models.
async fn fetch_openai_compat(client: &Client, url: &str, api_key: &str) -> Result<Vec<String>, String> {
    let mut ids = fetch_raw_oai(client, url, api_key).await?;
    ids.sort();
    Ok(ids)
}

async fn fetch_raw_oai(client: &Client, url: &str, api_key: &str) -> Result<Vec<String>, String> {
    let resp = client
        .get(url)
        .header("Authorization", format!("Bearer {api_key}"))
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }

    let list: OaiModelList = resp.json().await.map_err(|e| format!("parse error: {e}"))?;
    Ok(list.data.into_iter().map(|m| m.id).collect())
}

// ── Anthropic ──

#[derive(Deserialize)]
struct AnthropicModelList {
    data: Vec<AnthropicModel>,
}
#[derive(Deserialize)]
struct AnthropicModel {
    id: String,
}

async fn fetch_anthropic(client: &Client, api_key: &str) -> Result<Vec<String>, String> {
    let resp = client
        .get("https://api.anthropic.com/v1/models")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }

    let list: AnthropicModelList = resp.json().await.map_err(|e| format!("parse error: {e}"))?;
    let mut ids: Vec<String> = list.data.into_iter().map(|m| m.id).collect();
    ids.sort();
    Ok(ids)
}

// ── Google ──

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GoogleModelList {
    models: Option<Vec<GoogleModel>>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GoogleModel {
    name: String,
    #[serde(default)]
    supported_generation_methods: Vec<String>,
}

async fn fetch_google(client: &Client, api_key: &str) -> Result<Vec<String>, String> {
    let url = format!("https://generativelanguage.googleapis.com/v1beta/models?key={api_key}");
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }

    let list: GoogleModelList = resp.json().await.map_err(|e| format!("parse error: {e}"))?;
    let mut ids: Vec<String> = list
        .models
        .unwrap_or_default()
        .into_iter()
        .filter(|m| m.supported_generation_methods.iter().any(|method| method == "generateContent"))
        .map(|m| m.name.strip_prefix("models/").unwrap_or(&m.name).to_string())
        .collect();
    ids.sort();
    Ok(ids)
}

// ── Ollama ──

#[derive(Deserialize)]
struct OllamaTagList {
    models: Option<Vec<OllamaModel>>,
}
#[derive(Deserialize)]
struct OllamaModel {
    name: String,
}

async fn fetch_ollama(client: &Client) -> Result<Vec<String>, String> {
    let resp = client
        .get("http://localhost:11434/api/tags")
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }

    let list: OllamaTagList = resp.json().await.map_err(|e| format!("parse error: {e}"))?;
    let mut ids: Vec<String> = list.models.unwrap_or_default().into_iter().map(|m| m.name).collect();
    ids.sort();
    Ok(ids)
}
