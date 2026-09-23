//! OpenRouter chat-completions client.

use anyhow::{anyhow, Context as _, Result};
use serde_json::{json, Value};

use crate::stt::truncate;

const ENDPOINT: &str = "https://openrouter.ai/api/v1/chat/completions";

#[derive(Clone, Debug)]
pub struct ChatRequest {
    pub model: String,
    pub system: String,
    pub user: String,
    pub temperature: f32,
    pub max_tokens: u32,
    /// OpenRouter `reasoning` object, e.g. `{"effort":"minimal","exclude":true}`.
    pub reasoning: Option<Value>,
    pub json_mode: bool,
    /// Enable OpenRouter's web-search plugin.
    pub web: bool,
}

/// Where text completions go: OpenRouter (API key) or, in the desktop app, the local
/// Claude Code CLI (the user's own Claude subscription).
pub trait LlmCall: Send + Sync {
    fn complete<'a>(&'a self, req: &'a ChatRequest) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String>> + Send + 'a>>;
}

pub struct OpenRouter<'a> {
    pub http: &'a reqwest::Client,
    pub api_key: &'a str,
}

impl LlmCall for OpenRouter<'_> {
    fn complete<'a>(&'a self, req: &'a ChatRequest) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String>> + Send + 'a>> {
        Box::pin(chat(self.http, self.api_key, req))
    }
}

pub async fn chat(http: &reqwest::Client, api_key: &str, req: &ChatRequest) -> Result<String> {
    send(http, api_key, req, json!(req.user)).await
}

/// Same as `chat`, with a WAV recording attached to the user turn (multimodal models only,
/// e.g. Gemini). Used by the experimental one-step mode.
pub async fn chat_with_audio(http: &reqwest::Client, api_key: &str, req: &ChatRequest, wav: &[u8]) -> Result<String> {
    use base64::Engine as _;
    let data = base64::engine::general_purpose::STANDARD.encode(wav);
    let content = json!([
        {"type": "text", "text": req.user},
        {"type": "input_audio", "input_audio": {"data": data, "format": "wav"}},
    ]);
    send(http, api_key, req, content).await
}

async fn send(http: &reqwest::Client, api_key: &str, req: &ChatRequest, user_content: Value) -> Result<String> {
    if api_key.trim().is_empty() {
        return Err(anyhow!("OpenRouter API key is not set"));
    }
    let mut body = json!({
        "model": req.model,
        "messages": [
            {"role": "system", "content": req.system},
            {"role": "user", "content": user_content},
        ],
        "temperature": req.temperature,
        "max_tokens": req.max_tokens,
        "provider": {"sort": "latency"},
    });
    if let Some(r) = &req.reasoning {
        body["reasoning"] = r.clone();
    }
    if req.json_mode {
        body["response_format"] = json!({"type": "json_object"});
    }
    if req.web {
        body["plugins"] = json!([{"id": "web", "max_results": 5}]);
    }
    let resp = http
        .post(ENDPOINT)
        .bearer_auth(api_key.trim())
        .header("HTTP-Referer", "https://github.com/seyoon/sori")
        .header("X-Title", "Sori")
        .json(&body)
        .send()
        .await
        .context("OpenRouter request failed")?;
    let status = resp.status();
    let text = resp.text().await?;
    if !status.is_success() {
        return Err(anyhow!("OpenRouter error {}: {}", status.as_u16(), truncate(&text, 300)));
    }
    let v: Value = serde_json::from_str(&text).context("Could not parse OpenRouter response")?;
    if let Some(err) = v.get("error") {
        return Err(anyhow!("OpenRouter error: {}", truncate(&err.to_string(), 300)));
    }
    let content = v["choices"][0]["message"]["content"].as_str().unwrap_or("").to_string();
    if content.trim().is_empty() {
        return Err(anyhow!("The model returned an empty response ({})", req.model));
    }
    Ok(content)
}
