use serde::{Deserialize, Serialize};

/// A shortcut is the exact set of keys that must be held together, e.g. `["Fn", "ShiftLeft"]`.
/// Key names follow DOM `KeyboardEvent.code` where possible, plus `Fn`, `MouseMiddle`,
/// `Mouse4`, `Mouse5`.
pub type Shortcut = Vec<String>;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Shortcuts {
    pub dictate: Vec<Shortcut>,
    pub translate: Vec<Shortcut>,
    pub ask: Vec<Shortcut>,
}

impl Default for Shortcuts {
    /// Same defaults as Typeless on macOS.
    fn default() -> Self {
        Self {
            dictate: vec![vec!["Fn".into()]],
            translate: vec![vec!["Fn".into(), "ShiftLeft".into()]],
            ask: vec![vec!["Fn".into(), "Space".into()]],
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct TranslationTarget {
    /// BCP-47-ish code, e.g. `en-US`.
    pub code: String,
    /// Human name used in the prompt, e.g. `English (US)`.
    pub name: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Settings {
    pub elevenlabs_api_key: String,
    pub openrouter_api_key: String,

    pub stt_model: String,
    /// `elevenlabs` (cloud, default) | `local` (on-device whisper.cpp).
    pub stt_engine: String,
    /// On-device model id (see desktop `local_stt::MODELS`).
    pub local_model: String,
    /// Empty = auto-detect.
    pub stt_language: String,
    /// Model for dictation cleanup and translation.
    pub llm_model: String,
    /// Model for Ask anything / Help me write.
    pub ask_model: String,
    /// Text model backend: `openrouter` (API key) | `claude_code` (local Claude Code CLI,
    /// the user's own Claude subscription).
    pub llm_provider: String,
    /// Claude Code CLI model alias for cleanup/translation (`haiku` = fastest).
    pub claude_model: String,
    /// Claude Code CLI model alias for Ask anything.
    pub claude_ask_model: String,
    /// Claude Code `--effort` (`low` = fastest).
    pub claude_effort: String,
    /// `two_step` (STT → LLM, default) | `one_step` (experimental: audio straight into a
    /// multimodal LLM for dictation/translation).
    pub pipeline_mode: String,
    /// Audio-capable OpenRouter model used in one-step mode.
    pub one_step_model: String,

    pub shortcuts: Shortcuts,
    /// `toggle` (press to start, press again to finish) | `hybrid` (hold to talk; tap toggles)
    pub recording_mode: String,
    pub translation_targets: Vec<TranslationTarget>,
    pub active_translation_target: usize,

    /// `None` = system default input.
    pub microphone: Option<String>,
    /// Leave every generated text on the clipboard (so it can be pasted manually if the
    /// automatic paste didn't land). Off = restore the previous clipboard after pasting.
    pub copy_to_clipboard: bool,
    pub interaction_sounds: bool,
    pub mute_when_dictating: bool,

    pub launch_at_login: bool,
    pub show_in_dock: bool,
    /// `system` | `light` | `dark`
    pub appearance: String,
    /// `ko` | `en`
    pub interface_language: String,

    /// `forever` | `1y` | `1m` | `1w` | `24h` | `never`
    pub history_retention: String,

    /// Adapt tone to the frontmost app (email = formal, chat = casual, code = precise).
    pub per_app_tone: bool,
    /// Free-form personal style notes appended to every prompt.
    pub custom_instructions: String,
    /// Assume the speaker is a software developer: bias recognition toward developer
    /// vocabulary and restore misheard technical terms.
    /// `polished` (tighten into clear, concise writing) | `faithful` (only remove disfluencies).
    pub cleanup_style: String,
    pub dev_mode: bool,
    /// The user's usual stack, comma-separated (e.g. "Unity, C#, Rust, Tauri, React").
    pub tech_stack: String,
    /// Typing speed used to estimate "time saved".
    pub typing_wpm: u32,
    /// Writing by hand also costs time to organize thoughts and polish wording, which
    /// dictation + AI cleanup removes. Typing time is multiplied by this (1.0 = copy-typing).
    pub compose_factor: f32,
    /// Temporarily set "Press 🌐 key to" = Do Nothing while Sori runs (macOS).
    pub manage_fn_key: bool,
    pub onboarding_done: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            elevenlabs_api_key: String::new(),
            openrouter_api_key: String::new(),
            stt_model: "scribe_v2".into(),
            stt_engine: "elevenlabs".into(),
            local_model: "whisper-large-v3-turbo-q5_0".into(),
            stt_language: String::new(),
            llm_model: "google/gemini-3.8-flash".into(),
            ask_model: "google/gemini-3.8-flash".into(),
            llm_provider: "openrouter".into(),
            claude_model: "haiku".into(),
            claude_ask_model: "haiku".into(),
            claude_effort: "low".into(),
            pipeline_mode: "two_step".into(),
            one_step_model: "google/gemini-3.8-flash".into(),
            shortcuts: Shortcuts::default(),
            recording_mode: "toggle".into(),
            translation_targets: vec![TranslationTarget { code: "en-US".into(), name: "English (US)".into() }],
            active_translation_target: 0,
            microphone: None,
            copy_to_clipboard: true,
            interaction_sounds: true,
            mute_when_dictating: false,
            launch_at_login: false,
            show_in_dock: true,
            appearance: "system".into(),
            interface_language: "en".into(),
            history_retention: "forever".into(),
            per_app_tone: true,
            custom_instructions: String::new(),
            dev_mode: true,
            tech_stack: String::new(),
            typing_wpm: 40,
            compose_factor: 2.0,
            cleanup_style: "polished".into(),
            manage_fn_key: false,
            onboarding_done: false,
        }
    }
}

impl Settings {
    pub fn claude_code(&self) -> bool {
        self.llm_provider == "claude_code"
    }

    /// Model id for cleanup/translation on the selected backend.
    pub fn text_model(&self) -> String {
        if self.claude_code() { self.claude_model.clone() } else { self.llm_model.clone() }
    }

    /// Model id for Ask anything on the selected backend.
    pub fn ask_text_model(&self) -> String {
        if self.claude_code() { self.claude_ask_model.clone() } else { self.ask_model.clone() }
    }

    pub fn one_step(&self) -> bool {
        self.pipeline_mode == "one_step"
    }

    pub fn active_target(&self) -> TranslationTarget {
        self.translation_targets
            .get(self.active_translation_target)
            .or_else(|| self.translation_targets.first())
            .cloned()
            .unwrap_or(TranslationTarget { code: "en-US".into(), name: "English (US)".into() })
    }

    /// Retention window in milliseconds; `None` = keep forever, `Some(0)` = don't keep.
    pub fn retention_ms(&self) -> Option<i64> {
        const DAY: i64 = 24 * 60 * 60 * 1000;
        match self.history_retention.as_str() {
            "never" => Some(0),
            "24h" => Some(DAY),
            "1w" => Some(7 * DAY),
            "1m" => Some(30 * DAY),
            "1y" => Some(365 * DAY),
            _ => None,
        }
    }
}
