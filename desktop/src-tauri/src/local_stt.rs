//! On-device speech-to-text with whisper.cpp (via `whisper-rs`).
//!
//! Metal on macOS, Vulkan on Windows (see Cargo.toml). The model is downloaded once into
//! the app data dir, loaded on demand and kept in memory so each dictation only pays for
//! inference (~0.5 s for a 12 s clip on an M4 Pro).

use std::sync::Arc;
use std::time::Instant;

use anyhow::{anyhow, bail, Result};
use parking_lot::Mutex;
use sori_core::settings::Settings;

use crate::models::ModelStore;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperState};

struct Loaded {
    id: String,
    // `state` borrows nothing from `ctx` (it holds its own Arc), but keep ctx alive alongside.
    _ctx: WhisperContext,
    state: WhisperState,
}

pub struct LocalStt {
    pub store: Arc<ModelStore>,
    loaded: Mutex<Option<Loaded>>,
    /// Serializes model loading (startup preload vs. a dictation that arrives mid-load).
    load_lock: Mutex<()>,
}

pub struct Transcript {
    pub text: String,
    pub language: String,
    pub ms: u64,
}

impl LocalStt {
    pub fn new(store: Arc<ModelStore>) -> Self {
        whisper_rs::install_logging_hooks();
        Self { store, loaded: Mutex::new(None), load_lock: Mutex::new(()) }
    }

    pub fn is_installed(&self, id: &str) -> bool {
        self.store.is_installed(id)
    }

    pub fn loaded_id(&self) -> Option<String> {
        self.loaded.lock().as_ref().map(|l| l.id.clone())
    }

    /// Load (or switch to) a model. Blocking; ~6 s on an M4 Pro (Metal kernels are compiled at
    /// load time), so the app preloads it in the background. ~850 MB resident while loaded.
    pub fn ensure_loaded(&self, id: &str) -> Result<()> {
        let _loading = self.load_lock.lock();
        if self.loaded.lock().as_ref().map(|l| l.id == id).unwrap_or(false) {
            return Ok(());
        }
        if !self.is_installed(id) {
            if let Some(d) = self.store.active_download(id).filter(|d| d.state != "error" && d.state != "paused") {
                bail!("The speech model is still downloading ({}%) — try again in a moment", d.downloaded * 100 / d.total.max(1));
            }
            bail!("The on-device speech model isn't downloaded yet (Settings → AI)");
        }
        let path = self.store.path(id)?;
        let t = Instant::now();
        let mut params = WhisperContextParameters::default();
        params.use_gpu(true).flash_attn(true);
        let ctx = WhisperContext::new_with_params(&path, params).map_err(|e| anyhow!("Could not load the model: {e}"))?;
        let state = ctx.create_state().map_err(|e| anyhow!("Could not initialize the model: {e}"))?;
        *self.loaded.lock() = Some(Loaded { id: id.into(), _ctx: ctx, state });
        log::info!("local stt loaded {id} in {}ms", t.elapsed().as_millis());
        Ok(())
    }

    pub fn unload(&self) {
        if self.loaded.lock().take().is_some() {
            log::info!("local stt unloaded");
        }
    }

    /// Transcribe 16 kHz mono samples. `language`: None/"" = auto-detect.
    pub fn transcribe(&self, id: &str, samples: &[f32], language: Option<&str>, prompt: &str) -> Result<Transcript> {
        self.ensure_loaded(id)?;
        let t = Instant::now();
        let mut guard = self.loaded.lock();
        let loaded = guard.as_mut().ok_or_else(|| anyhow!("Model not loaded"))?;

        let mut p = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(8) as i32;
        p.set_n_threads(threads);
        let lang = language.filter(|l| !l.is_empty()).unwrap_or("auto");
        p.set_language(Some(lang));
        p.set_no_timestamps(true);
        p.set_no_context(true);
        p.set_suppress_blank(true);
        p.set_print_special(false);
        p.set_print_progress(false);
        p.set_print_realtime(false);
        p.set_print_timestamps(false);
        if !prompt.is_empty() {
            p.set_initial_prompt(prompt);
        }
        loaded.state.full(p, samples).map_err(|e| anyhow!("Speech recognition failed: {e}"))?;

        let mut text = String::new();
        for seg in loaded.state.as_iter() {
            if let Ok(s) = seg.to_str_lossy() {
                text.push_str(s.trim());
                text.push(' ');
            }
        }
        let language = whisper_rs::get_lang_str(loaded.state.full_lang_id_from_state()).unwrap_or("").to_string();
        Ok(Transcript { text: clean_hallucinations(text.trim()), language, ms: t.elapsed().as_millis() as u64 })
    }

}

/// Whisper's initial prompt: biases spelling toward the user's vocabulary (English terms in
/// Latin script). Phrased as a plain sentence — a "용어:" label style leaked into transcripts
/// in testing (FLEURS-ko CER 2.76% → 4.17%), this form stays at ~3.0% with no leaks.
pub fn build_prompt(s: &Settings, dictionary: &[String]) -> String {
    let korean = match s.stt_language.as_str() {
        "ko" => true,
        "" => s.interface_language == "ko",
        _ => false,
    };
    build_prompt_lang(s, dictionary, korean)
}

/// Prompt sentence in Korean or English (the prompt's language nudges the decoder's style).
pub fn build_prompt_lang(s: &Settings, dictionary: &[String], korean: bool) -> String {
    let mut terms: Vec<String> = dictionary.to_vec();
    if s.dev_mode {
        terms.extend(sori_core::devterms::tech_stack(s));
        terms.extend(PROMPT_DEV_TERMS.iter().map(|t| t.to_string()));
    }
    let mut seen = std::collections::HashSet::new();
    terms.retain(|t| seen.insert(t.to_lowercase()));
    let mut list = String::new();
    for t in terms {
        if list.chars().count() + t.chars().count() > 360 {
            break;
        }
        if !list.is_empty() {
            list.push_str(", ");
        }
        list.push_str(&t);
    }
    if list.is_empty() {
        return String::new();
    }
    let (lead, tail) = PROMPTS[if korean { 0 } else { 1 }];
    format!("{lead}{list}{tail}")
}

const PROMPTS: [(&str, &str); 2] = [
    ("오늘 회의에서는 ", " 같은 용어를 섞어서 이야기했습니다."),
    ("In today's meeting we talked about ", " and related topics."),
];

/// Short list of terms Korean speakers say often and Whisper tends to write in Hangul.
const PROMPT_DEV_TERMS: &[&str] = &[
    "API", "React", "React Query", "useEffect", "TypeScript", "JavaScript", "Next.js", "Node.js", "Python", "Rust",
    "Git", "GitHub", "PR", "pull request", "commit", "merge", "rebase", "Docker", "Docker Compose", "Kubernetes",
    "PostgreSQL", "Redis", "JSON", "YAML", ".env", "CI/CD", "deploy", "LLM", "Claude Code", "Cursor", "Tauri",
    "Supabase", "Vercel", "AWS", "SDK", "CLI", "OAuth", "WebSocket", "localhost", "refactor", "prompt", "token",
];

/// Drop whisper's classic silence hallucinations, bracketed sound tags and any echo of the
/// initial prompt.
fn clean_hallucinations(text: &str) -> String {
    let mut t = text.to_string();
    for (lead, tail) in PROMPTS {
        if let (Some(a), Some(b)) = (t.find(lead.trim()), t.find(tail.trim())) {
            if a < b {
                t.replace_range(a..b + tail.trim().len(), "");
            }
        }
    }
    let mut t = t.trim().to_string();
    for (open, close) in [('[', ']'), ('(', ')')] {
        // Remove whole-transcript tags like "[음악]" or "(박수)".
        if t.starts_with(open) && t.ends_with(close) && t.chars().filter(|c| *c == open).count() == 1 {
            t.clear();
        }
    }
    const JUNK: &[&str] = &[
        "시청해주셔서 감사합니다", "시청해 주셔서 감사합니다", "구독과 좋아요 부탁드립니다", "MBC 뉴스", "Thank you for watching",
        "Thanks for watching", "자막 제공",
    ];
    if JUNK.iter().any(|j| t.contains(j)) && t.chars().count() < 40 {
        t.clear();
    }
    t.trim().to_string()
}
