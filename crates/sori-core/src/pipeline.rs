//! STT → LLM pipeline.

use std::time::Instant;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::llm::{self, ChatRequest, LlmCall};
use crate::settings::{Settings, TranslationTarget};
use crate::{prompts, stt, text};

/// What was on screen when the recording started.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Context {
    pub app_name: String,
    pub bundle_id: String,
    pub window_title: String,
    /// `Some(true)` if an editable text element had focus, `Some(false)` if focus was on
    /// something non-editable, `None` if unknown.
    pub field_focused: Option<bool>,
    pub selected_text: Option<String>,
    pub selection_editable: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Mode {
    Dictate,
    Translate { target: TranslationTarget },
    Ask,
}

impl Mode {
    pub fn key(&self) -> &'static str {
        match self {
            Mode::Dictate => "dictate",
            Mode::Translate { .. } => "translate",
            Mode::Ask => "ask",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AskAction {
    /// Paste at the cursor.
    Insert,
    /// Paste over the current selection.
    Replace,
    /// Show in a card.
    Answer,
    OpenUrl,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Outcome {
    pub raw: String,
    pub output: String,
    pub action: AskAction,
    pub url: Option<String>,
    pub language: String,
    pub stt_ms: u64,
    pub llm_ms: u64,
    /// Why the raw transcript was used / something degraded. Stable codes the app localizes:
    /// `assistant_reply`, `llm_failed: <detail>`, `local_stt_failed: <detail>`.
    pub fallback_reason: Option<String>,
}

pub struct Processed {
    pub output: String,
    pub action: AskAction,
    pub url: Option<String>,
    pub llm_ms: u64,
    pub fallback_reason: Option<String>,
}

pub async fn transcribe(
    http: &reqwest::Client,
    s: &Settings,
    dictionary: &[String],
    wav: Vec<u8>,
) -> Result<(stt::SttResult, u64)> {
    let t = Instant::now();
    let lang = if s.stt_language.is_empty() { None } else { Some(s.stt_language.as_str()) };
    let keyterms = crate::devterms::stt_keyterms(s, dictionary);
    let r = stt::transcribe(http, &s.elevenlabs_api_key, &s.stt_model, wav, lang, &keyterms).await?;
    Ok((r, t.elapsed().as_millis() as u64))
}

pub async fn process_text(
    llm: &dyn LlmCall,
    s: &Settings,
    dictionary: &[String],
    raw: &str,
    mode: &Mode,
    ctx: &Context,
) -> Result<Processed> {
    let t = Instant::now();
    let minimal = Some(json!({"effort": "minimal", "exclude": true}));
    match mode {
        Mode::Dictate => {
            let local = s.local_llm();
            let (system, user) = if local { prompts::dictate_local(s, ctx, dictionary, raw) } else { prompts::dictate(s, ctx, dictionary, raw) };
            let req = ChatRequest {
                model: s.text_model(),
                system,
                user,
                temperature: if local { 0.1 } else { 0.2 },
                max_tokens: if local { 1024 } else { 4096 },
                reasoning: minimal,
                json_mode: false,
                web: false,
                examples: if local { prompts::local_examples() } else { vec![] },
            };
            match llm.complete(&req).await {
                Ok(out) => {
                    let out = text::clean_llm_output(&out);
                    if text::looks_like_assistant_reply(raw, &out) {
                        Ok(Processed {
                            output: raw.to_string(),
                            action: AskAction::Insert,
                            url: None,
                            llm_ms: t.elapsed().as_millis() as u64,
                            fallback_reason: Some("assistant_reply".into()),
                        })
                    } else {
                        Ok(Processed { output: out, action: AskAction::Insert, url: None, llm_ms: t.elapsed().as_millis() as u64, fallback_reason: None })
                    }
                }
                // Network/LLM failure: still deliver the raw transcript rather than losing it.
                Err(e) => Ok(Processed {
                    output: raw.to_string(),
                    action: AskAction::Insert,
                    url: None,
                    llm_ms: t.elapsed().as_millis() as u64,
                    fallback_reason: Some(format!("llm_failed: {e}")),
                }),
            }
        }
        Mode::Translate { target } => {
            let (system, user, examples) = if s.local_llm() {
                prompts::translate_local(s, dictionary, raw, &target.name)
            } else {
                let (system, user) = prompts::translate(s, ctx, dictionary, raw, &target.name);
                (system, user, vec![])
            };
            let req = ChatRequest {
                model: s.text_model(),
                system,
                user,
                temperature: if s.local_llm() { 0.1 } else { 0.3 },
                max_tokens: 4096,
                reasoning: minimal,
                json_mode: false,
                web: false,
                examples,
            };
            let out = text::clean_llm_output(&llm.complete(&req).await?);
            Ok(Processed { output: out, action: AskAction::Insert, url: None, llm_ms: t.elapsed().as_millis() as u64, fallback_reason: None })
        }
        Mode::Ask => {
            let (system, user) = prompts::ask(s, ctx, dictionary, raw);
            let req = ChatRequest {
                model: s.ask_text_model(),
                system,
                user,
                temperature: 0.4,
                max_tokens: 8192,
                reasoning: Some(json!({"effort": "low", "exclude": true})),
                json_mode: true,
                web: false,
                examples: vec![],
            };
            let content = llm.complete(&req).await?;
            let v = parse_json_loose(&content).unwrap_or_else(|| json!({"action": "answer", "text": content}));
            let mut action = match v["action"].as_str().unwrap_or("answer") {
                "replace" => AskAction::Replace,
                "insert" => AskAction::Insert,
                "open_url" => AskAction::OpenUrl,
                _ => AskAction::Answer,
            };
            let mut out = v["text"].as_str().unwrap_or("").trim().to_string();
            let url = v["url"].as_str().filter(|u| u.starts_with("http")).map(str::to_string);
            // Enforce the rules the model might ignore.
            let has_editable_selection =
                ctx.selection_editable && ctx.selected_text.as_deref().map_or(false, |t| !t.trim().is_empty());
            if action == AskAction::Replace && !has_editable_selection {
                action = AskAction::Answer;
            }
            if action == AskAction::Insert && ctx.field_focused == Some(false) {
                action = AskAction::Answer;
            }
            if action == AskAction::OpenUrl && url.is_none() {
                action = AskAction::Answer;
            }
            if action == AskAction::Answer && (v["needs_web"].as_bool() == Some(true) || out.is_empty()) {
                let (system, user) = prompts::ask_web(ctx, raw);
                let req = ChatRequest {
                    model: s.ask_text_model(),
                    system,
                    user,
                    temperature: 0.3,
                    max_tokens: 8192,
                    reasoning: Some(json!({"effort": "low", "exclude": true})),
                    json_mode: false,
                    web: true,
                    examples: vec![],
                };
                out = llm.complete(&req).await?.trim().to_string();
            }
            Ok(Processed { output: out, action, url, llm_ms: t.elapsed().as_millis() as u64, fallback_reason: None })
        }
    }
}

fn parse_json_loose(s: &str) -> Option<Value> {
    let t = text::clean_llm_output(s);
    if let Ok(v) = serde_json::from_str::<Value>(&t) {
        return Some(v);
    }
    let (a, b) = (t.find('{')?, t.rfind('}')?);
    serde_json::from_str(&t[a..=b]).ok()
}

/// Experimental one-step mode: audio goes straight into a multimodal LLM (Dictate/Translate).
/// There's no separate transcript, so `raw` is empty.
pub async fn run_one_step(
    http: &reqwest::Client,
    s: &Settings,
    dictionary: &[String],
    wav: &[u8],
    mode: &Mode,
    ctx: &Context,
) -> Result<Outcome> {
    let t = Instant::now();
    let base = match mode {
        Mode::Dictate => prompts::dictate(s, ctx, dictionary, "").0,
        Mode::Translate { target } => prompts::translate(s, ctx, dictionary, "", &target.name).0,
        Mode::Ask => anyhow::bail!("one-step mode doesn't handle Ask"),
    };
    let (system, user) = prompts::from_audio(s, ctx, dictionary, base);
    let req = ChatRequest {
        model: s.one_step_model.clone(),
        system,
        user,
        temperature: 0.2,
        max_tokens: 4096,
        reasoning: Some(json!({"effort": "minimal", "exclude": true})),
        json_mode: false,
        web: false,
        examples: vec![],
    };
    let out = text::clean_llm_output(&llm::chat_with_audio(http, &s.openrouter_api_key, &req, wav).await?);
    Ok(Outcome {
        raw: String::new(),
        output: out,
        action: AskAction::Insert,
        url: None,
        language: String::new(),
        stt_ms: 0,
        llm_ms: t.elapsed().as_millis() as u64,
        fallback_reason: None,
    })
}

/// Full pipeline for one recording.
pub async fn run(
    http: &reqwest::Client,
    llm: &dyn LlmCall,
    s: &Settings,
    dictionary: &[String],
    wav: Vec<u8>,
    mode: &Mode,
    ctx: &Context,
) -> Result<Outcome> {
    if s.one_step() && *mode != Mode::Ask {
        match run_one_step(http, s, dictionary, &wav, mode, ctx).await {
            Ok(o) => return Ok(o),
            Err(e) => log::warn!("one-step failed, falling back to STT → LLM: {e:#}"),
        }
    }
    let (stt, stt_ms) = transcribe(http, s, dictionary, wav).await?;
    if stt.text.trim().is_empty() {
        return Ok(Outcome {
            raw: String::new(),
            output: String::new(),
            action: AskAction::Insert,
            url: None,
            language: stt.language_code,
            stt_ms,
            llm_ms: 0,
            fallback_reason: None,
        });
    }
    let p = process_text(llm, s, dictionary, &stt.text, mode, ctx).await?;
    Ok(Outcome {
        raw: stt.text,
        output: p.output,
        action: p.action,
        url: p.url,
        language: stt.language_code,
        stt_ms,
        llm_ms: p.llm_ms,
        fallback_reason: p.fallback_reason,
    })
}
