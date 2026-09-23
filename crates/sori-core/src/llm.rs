//! OpenRouter chat-completions client.

use anyhow::{anyhow, Context as _, Result};
use serde_json::{json, Value};

use crate::stt::truncate;

const ENDPOINT: &str = "https://openrouter.ai/api/v1/chat/completions";

#[derive(Clone, Debug, Default)]
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
    /// Few-shot (input, output) pairs sent as prior turns — small local models need them.
    pub examples: Vec<(String, String)>,
}

/// system + few-shot turns + user, as OpenAI-style messages.
fn messages(req: &ChatRequest, user_content: Value) -> Value {
    let mut m = vec![json!({"role": "system", "content": req.system})];
    for (i, o) in &req.examples {
        m.push(json!({"role": "user", "content": i}));
        m.push(json!({"role": "assistant", "content": o}));
    }
    m.push(json!({"role": "user", "content": user_content}));
    Value::Array(m)
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
        "messages": messages(req, user_content),
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

/// A local OpenAI-compatible server (llama.cpp `llama-server`) — the on-device text model.
/// `cache_prompt` keeps the shared prompt prefix in the KV cache between requests, so only
/// the new transcript has to be processed; thinking is switched off for hybrid models.
pub struct LocalServer<'a> {
    pub http: &'a reqwest::Client,
    /// e.g. `http://127.0.0.1:39281`
    pub base_url: String,
    /// Bearer token the server was started with (empty = none).
    pub api_key: String,
}

impl LocalServer<'_> {
    pub async fn chat(&self, req: &ChatRequest) -> Result<String> {
        let mut body = json!({
            "messages": messages(req, json!(req.user)),
            "temperature": req.temperature,
            "max_tokens": req.max_tokens,
            "cache_prompt": true,
            "chat_template_kwargs": {"enable_thinking": false},
        });
        if req.json_mode {
            body["response_format"] = json!({"type": "json_object"});
        }
        let mut post = self.http.post(format!("{}/v1/chat/completions", self.base_url.trim_end_matches('/')));
        if !self.api_key.is_empty() {
            post = post.bearer_auth(&self.api_key);
        }
        let resp = post
            .json(&body)
            .send()
            .await
            .context("The on-device text model isn't running")?;
        let status = resp.status();
        let text = resp.text().await?;
        if !status.is_success() {
            return Err(anyhow!("On-device model error {}: {}", status.as_u16(), truncate(&text, 300)));
        }
        let v: Value = serde_json::from_str(&text).context("Could not parse the on-device model response")?;
        let raw = v["choices"][0]["message"]["content"].as_str().unwrap_or("");
        let content = strip_think(raw);
        if content.trim().is_empty() {
            return Err(anyhow!("The on-device model returned an empty response"));
        }
        Ok(content)
    }
}

impl LlmCall for LocalServer<'_> {
    fn complete<'a>(&'a self, req: &'a ChatRequest) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String>> + Send + 'a>> {
        Box::pin(self.chat(req))
    }
}

/// Drop a `<think>…</think>` block some small models still emit.
fn strip_think(s: &str) -> String {
    match (s.find("<think>"), s.find("</think>")) {
        (Some(a), Some(b)) if a < b => format!("{}{}", &s[..a], &s[b + "</think>".len()..]).trim().to_string(),
        (None, Some(b)) => s[b + "</think>".len()..].trim().to_string(),
        _ => s.trim().to_string(),
    }
}

