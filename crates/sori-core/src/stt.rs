//! ElevenLabs Speech-to-Text (Scribe) client.

use anyhow::{anyhow, Context as _, Result};
use serde::Deserialize;

const ENDPOINT: &str = "https://api.elevenlabs.io/v1/speech-to-text";

#[derive(Debug, Clone, Default)]
pub struct SttResult {
    pub text: String,
    pub language_code: String,
    pub duration_secs: f32,
}

#[derive(Deserialize)]
struct Resp {
    #[serde(default)]
    text: String,
    #[serde(default)]
    language_code: String,
    #[serde(default)]
    audio_duration_secs: Option<f32>,
}

/// Keyterm limits for Scribe v2 batch: ≤1000 terms, <50 chars, ≤5 words each.
pub fn sanitize_keyterms(terms: &[String]) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    for t in terms {
        let t = t.trim();
        if t.is_empty() || t.chars().count() >= 50 || t.split_whitespace().count() > 5 {
            continue;
        }
        if !out.iter().any(|o| o.eq_ignore_ascii_case(t)) {
            out.push(t.to_string());
        }
        if out.len() >= 1000 {
            break;
        }
    }
    out
}

pub async fn transcribe(
    http: &reqwest::Client,
    api_key: &str,
    model: &str,
    wav: Vec<u8>,
    language: Option<&str>,
    keyterms: &[String],
) -> Result<SttResult> {
    if api_key.trim().is_empty() {
        return Err(anyhow!("ElevenLabs API key is not set"));
    }
    let part = reqwest::multipart::Part::bytes(wav).file_name("audio.wav").mime_str("audio/wav")?;
    let mut form = reqwest::multipart::Form::new()
        .text("model_id", model.to_string())
        .text("tag_audio_events", "false")
        .part("file", part);
    if let Some(l) = language.filter(|l| !l.is_empty()) {
        form = form.text("language_code", l.to_string());
    }
    for k in sanitize_keyterms(keyterms) {
        form = form.text("keyterms", k);
    }
    let resp = http
        .post(ENDPOINT)
        .header("xi-api-key", api_key.trim())
        .multipart(form)
        .send()
        .await
        .context("ElevenLabs request failed")?;
    let status = resp.status();
    let body = resp.text().await?;
    if !status.is_success() {
        return Err(anyhow!("ElevenLabs STT error {}: {}", status.as_u16(), truncate(&body, 300)));
    }
    let r: Resp = serde_json::from_str(&body).context("Could not parse ElevenLabs response")?;
    Ok(SttResult {
        text: r.text.trim().to_string(),
        language_code: r.language_code,
        duration_secs: r.audio_duration_secs.unwrap_or(0.0),
    })
}

pub(crate) fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        s.chars().take(n).collect::<String>() + "…"
    }
}
