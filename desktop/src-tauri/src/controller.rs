//! Orchestrates a dictation session: hotkey → record → STT/LLM → deliver → history.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use parking_lot::{Mutex, RwLock};
use serde::Serialize;
use serde_json::json;
use sori_core::hotkey::{Action, Engine};
use sori_core::settings::{Settings, TranslationTarget};
use sori_core::store::{HistoryEntry, Store};
use sori_core::{audio, pipeline, text, AskAction, Context, Mode};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition};

use crate::admin::{self, Admin};
use crate::local_llm::{LlmServer, LocalLlm};
use crate::local_stt::{self, LocalStt};
use crate::models::ModelStore;
use crate::platform::{self, Captured, HotkeyEvent};
use crate::recorder::{Recorder, Recording};

const MAX_RECORDING: Duration = Duration::from_secs(9 * 60);
const WARN_AT: Duration = Duration::from_secs(8 * 60);
const MIN_RECORDING: Duration = Duration::from_millis(250);
pub const HUD_W: f64 = 300.0;
pub const HUD_H: f64 = 84.0;

struct Session {
    id: u64,
    action: Action,
    target: TranslationTarget,
    started: Instant,
    started_ms: i64,
    hands_free: bool,
    capture: Option<JoinHandle<Captured>>,
    target_pid: Arc<AtomicI32>,
}

#[derive(Serialize, Clone, Default)]
pub struct HudState {
    /// `recording` | `processing` | `message` | `hidden`
    pub phase: String,
    pub mode: String,
    pub hands_free: bool,
    pub started_at: i64,
    pub target: Option<TranslationTarget>,
    pub targets: Vec<TranslationTarget>,
    pub target_idx: usize,
    pub message: Option<String>,
    /// `info` | `error` | `success`
    pub tone: String,
    pub countdown: Option<u64>,
}

#[derive(Serialize, Clone)]
pub struct CardContent {
    /// `answer` | `result`
    pub kind: String,
    pub title: String,
    pub text: String,
    pub question: String,
    /// The text was also put on the clipboard.
    pub copied: bool,
}

pub struct App {
    pub handle: AppHandle,
    pub settings: RwLock<Settings>,
    pub store: Mutex<Store>,
    pub recorder: Recorder,
    pub admin: Admin,
    pub models: Arc<ModelStore>,
    pub local_stt: Arc<LocalStt>,
    pub llm_server: Arc<LlmServer>,
    pub http: reqwest::Client,
    pub engine: Arc<Mutex<Engine>>,
    pub data_dir: PathBuf,
    pub config_path: PathBuf,
    session: Mutex<Option<Session>>,
    next_id: AtomicU64,
    muted: AtomicBool,
    pub tap_ready: AtomicBool,
    pub saved_fn_usage: Mutex<Option<i32>>,
    pub mic_test: AtomicBool,
    last_target_pid: AtomicI32,
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

pub fn default_settings() -> Settings {
    let mut s = Settings::default();
    // No Fn key on Windows keyboards: hold Ctrl+Win (Typeless / Wispr Flow convention).
    if cfg!(windows) {
        let c = |keys: &[&str]| keys.iter().map(|k| k.to_string()).collect::<Vec<_>>();
        s.shortcuts.dictate = vec![c(&["ControlLeft", "MetaLeft"])];
        s.shortcuts.translate = vec![c(&["ControlLeft", "MetaLeft", "ShiftLeft"])];
        s.shortcuts.ask = vec![c(&["ControlLeft", "MetaLeft", "Space"])];
    }
    // Defaults are fully on-device (Whisper + a small local text model): no keys, nothing
    // leaves the computer. Cloud engines are opt-in in Settings → AI.
    s
}

/// Pick the UI language's string. Everything user-facing on the Rust side goes through this.
pub fn tr<'a>(s: &Settings, en: &'a str, ko: &'a str) -> &'a str {
    if s.interface_language == "ko" {
        ko
    } else {
        en
    }
}

/// Human-readable text for a pipeline note code (`assistant_reply`, `llm_failed: …`,
/// `local_stt_failed: …`). History stores the code; the UI localizes it the same way.
pub fn note_text(s: &Settings, code: &str) -> String {
    let (key, detail) = code.split_once(": ").unwrap_or((code, ""));
    let base = match key {
        "assistant_reply" => tr(s, "The AI answered instead of cleaning up — inserted your words as spoken", "AI가 정리 대신 답변을 해서 원문을 그대로 넣었어요"),
        "llm_failed" => tr(s, "AI cleanup failed — inserted the raw transcript", "AI 다듬기에 실패해 원문을 그대로 넣었어요"),
        "local_stt_failed" => tr(s, "On-device recognition failed — used ElevenLabs instead", "로컬 음성 인식을 쓸 수 없어 ElevenLabs로 처리했어요"),
        _ => return code.to_string(),
    };
    if detail.is_empty() {
        base.to_string()
    } else {
        format!("{base} ({detail})")
    }
}

pub fn load_settings(path: &PathBuf) -> Settings {
    match std::fs::read_to_string(path) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_else(|e| {
            log::error!("settings parse error: {e}; using defaults");
            default_settings()
        }),
        Err(_) => default_settings(),
    }
}

impl App {
    pub fn new(handle: AppHandle, data_dir: PathBuf, config_path: PathBuf) -> anyhow::Result<Arc<Self>> {
        std::fs::create_dir_all(data_dir.join("audio"))?;
        let admin = Admin::new(config_path.with_file_name("admin.key"));
        let mut settings = load_settings(&config_path);
        // Cloud engines + API keys are owner-only: without admin mode, everything is on-device.
        if admin.is_unlocked() {
            admin.fill_default_keys(&mut settings);
        }
        admin::enforce_policy(&mut settings, admin.is_unlocked());
        let store = Store::open(&data_dir.join("sori.db"))?;
        let engine = Arc::new(Mutex::new(Engine::new(&settings.shortcuts)));
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(90))
            .pool_idle_timeout(Duration::from_secs(300))
            .tcp_keepalive(Duration::from_secs(30))
            .build()?;
        let models = Arc::new(ModelStore::new(data_dir.join("models")));
        let app = Arc::new(Self {
            handle,
            settings: RwLock::new(settings),
            store: Mutex::new(store),
            recorder: Recorder::spawn(),
            admin,
            local_stt: Arc::new(LocalStt::new(models.clone())),
            llm_server: Arc::new(LlmServer::new(models.clone(), data_dir.join("llama-server.log"))),
            models,
            http,
            engine,
            data_dir,
            config_path,
            session: Mutex::new(None),
            next_id: AtomicU64::new(1),
            muted: AtomicBool::new(false),
            tap_ready: AtomicBool::new(false),
            saved_fn_usage: Mutex::new(None),
            mic_test: AtomicBool::new(false),
            last_target_pid: AtomicI32::new(0),
        });
        app.save_settings_file()?;
        app.recorder.prepare(app.settings.read().microphone.clone());
        app.sync_local_stt();
        app.sync_local_llm();
        Ok(app)
    }

    pub fn save_settings_file(&self) -> anyhow::Result<()> {
        if let Some(dir) = self.config_path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let json = serde_json::to_string_pretty(&*self.settings.read())?;
        std::fs::write(&self.config_path, json)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&self.config_path, std::fs::Permissions::from_mode(0o600));
        }
        Ok(())
    }

    /// Load the on-device model in the background when it's the selected engine (so the
    /// first dictation is fast); free it otherwise.
    pub fn sync_local_stt(self: &Arc<Self>) {
        let s = self.settings.read().clone();
        if s.stt_engine == "local" {
            // Selected but missing (first launch, or just switched in Settings): fetch it now,
            // resuming any partial download — unless the user paused/deleted it on purpose.
            if self.models.should_auto_download(&s.local_model) {
                self.start_model_download(&s.local_model);
            }
            if self.local_stt.is_installed(&s.local_model) {
                let stt = self.local_stt.clone();
                std::thread::spawn(move || {
                    if let Err(e) = stt.ensure_loaded(&s.local_model) {
                        log::error!("local stt preload failed: {e:#}");
                    }
                });
            }
        } else {
            self.local_stt.unload();
        }
    }

    /// Same for the text model: download if needed, start llama-server and prime its prompt
    /// cache in the background; stop it (freeing ~3 GB) when a cloud model is selected.
    pub fn sync_local_llm(self: &Arc<Self>) {
        let s = self.settings.read().clone();
        if !s.local_llm() {
            let srv = self.llm_server.clone();
            tauri::async_runtime::spawn(async move { srv.stop().await });
            return;
        }
        let id = s.local_llm_model.clone();
        if self.models.should_auto_download(&id) {
            self.start_model_download(&id);
        }
        if self.models.is_installed(&id) {
            let app = self.clone();
            tauri::async_runtime::spawn(async move {
                let dict = app.store.lock().dictionary_terms().unwrap_or_default();
                let (system, user) = sori_core::prompts::dictate_local(&s, &sori_core::Context::default(), &dict, "음 테스트");
                let req = sori_core::llm::ChatRequest {
                    model: id.clone(),
                    system,
                    user,
                    max_tokens: 1,
                    examples: sori_core::prompts::local_examples(),
                    ..Default::default()
                };
                let t = Instant::now();
                match app.llm_server.complete(&id, &req).await {
                    Ok(_) => log::info!("local llm primed in {}ms", t.elapsed().as_millis()),
                    Err(e) => log::error!("local llm start failed: {e:#}"),
                }
            });
        }
    }

    /// Download/resume a model in the background, then load it if it's the selected one.
    pub fn start_model_download(self: &Arc<Self>, id: &str) {
        let app = self.clone();
        let id = id.to_string();
        tauri::async_runtime::spawn(async move {
            let h = app.handle.clone();
            let res = app
                .models
                .download(&app.http, &id, move |d| {
                    let _ = h.emit("local-model-download", d);
                })
                .await;
            match res {
                Ok(()) if app.models.is_installed(&id) => {
                    log::info!("model {id} ready");
                    app.sync_local_stt();
                    app.sync_local_llm();
                }
                Ok(()) => {}
                Err(e) => log::error!("model download failed: {e:#}"),
            }
        });
    }

    /// Speech → text with the selected engine. Local failures fall back to ElevenLabs.
    async fn transcribe(&self, settings: &Settings, dict: &[String], wav: Vec<u8>) -> anyhow::Result<(sori_core::stt::SttResult, u64, Option<String>)> {
        if settings.stt_engine == "local" {
            let stt = self.local_stt.clone();
            let model = settings.local_model.clone();
            let lang = settings.stt_language.clone();
            let prompt = local_stt::build_prompt(settings, dict);
            let wav2 = wav.clone();
            let res = tauri::async_runtime::spawn_blocking(move || {
                let (samples, rate) = audio::decode_wav(&wav2).ok_or_else(|| anyhow::anyhow!("Could not decode audio"))?;
                let samples = if rate == audio::TARGET_RATE { samples } else { audio::resample_to_16k(&samples, rate) };
                stt.transcribe(&model, &samples, Some(&lang), &prompt)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
            .and_then(|r| r);
            match res {
                Ok(t) => {
                    return Ok((sori_core::stt::SttResult { text: t.text, language_code: t.language, duration_secs: 0.0 }, t.ms, None))
                }
                Err(e) if !settings.elevenlabs_api_key.trim().is_empty() => {
                    log::warn!("local stt failed, using ElevenLabs: {e:#}");
                    let (r, ms) = pipeline::transcribe(&self.http, settings, dict, wav).await?;
                    return Ok((r, ms, Some(format!("local_stt_failed: {e}"))));
                }
                Err(e) => return Err(e),
            }
        }
        let (r, ms) = pipeline::transcribe(&self.http, settings, dict, wav).await?;
        Ok((r, ms, None))
    }

    /// The text model selected in settings: on-device llama.cpp or the OpenRouter API.
    pub fn llm<'a>(&'a self, s: &'a Settings) -> Box<dyn sori_core::llm::LlmCall + 'a> {
        if s.local_llm() {
            Box::new(LocalLlm { server: self.llm_server.clone(), model: s.local_llm_model.clone() })
        } else {
            Box::new(sori_core::llm::OpenRouter { http: &self.http, api_key: &s.openrouter_api_key })
        }
    }

    /// Keep HTTP/TLS connections to the cloud APIs warm so the first request after idle is
    /// fast, and make sure the on-device text model is up when it's selected.
    pub fn warm_up(self: &Arc<Self>) {
        let s = self.settings.read().clone();
        if s.local_llm() && self.models.is_installed(&s.local_llm_model) && self.llm_server.loaded_model().is_none() {
            let srv = self.llm_server.clone();
            let id = s.local_llm_model.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = srv.ensure_running(&id).await {
                    log::warn!("local llm warm-up: {e:#}");
                }
            });
        }
        if s.stt_engine == "local" && !s.local_llm() && s.pipeline_mode != "one_step" && s.openrouter_api_key.is_empty() {
            return;
        }
        let app = self.clone();
        tauri::async_runtime::spawn(async move {
            let _ = app.http.head("https://api.elevenlabs.io/").send().await;
            let _ = app.http.head("https://openrouter.ai/api/v1/models").send().await;
        });
    }

    // ------------------------------------------------------------------ hotkeys

    pub fn run_hotkey_loop(self: &Arc<Self>, rx: std::sync::mpsc::Receiver<HotkeyEvent>) {
        let app = self.clone();
        std::thread::Builder::new()
            .name("sori-controller".into())
            .spawn(move || {
                while let Ok(ev) = rx.recv() {
                    app.on_hotkey(ev);
                }
            })
            .expect("spawn controller");
    }

    fn on_hotkey(self: &Arc<Self>, ev: HotkeyEvent) {
        match ev {
            HotkeyEvent::Start(a) => self.start(a),
            HotkeyEvent::Switch(a) => self.switch(a),
            HotkeyEvent::HandsFree => {
                if let Some(s) = self.session.lock().as_mut() {
                    s.hands_free = true;
                }
                self.emit_recording_state();
            }
            HotkeyEvent::Stop => self.finish(),
            HotkeyEvent::Cancel => self.cancel(),
            HotkeyEvent::Recorded(combo) => {
                let _ = self.handle.emit_to("main", "shortcut-recorded", combo);
            }
            HotkeyEvent::TapReady => {
                self.tap_ready.store(true, Ordering::SeqCst);
                let _ = self.handle.emit("permissions-changed", ());
            }
            HotkeyEvent::TapUnavailable => {
                self.tap_ready.store(false, Ordering::SeqCst);
                let _ = self.handle.emit("permissions-changed", ());
            }
        }
    }

    /// Dev hook (SIGUSR1/SIGUSR2): start or finish a session without the keyboard.
    pub fn debug_toggle(self: &Arc<Self>, action: Action) {
        if self.session.lock().is_some() {
            self.engine.lock().reset();
            self.finish();
        } else {
            self.start(action);
            if let Some(s) = self.session.lock().as_mut() {
                s.hands_free = true;
            }
            self.emit_recording_state();
        }
    }

    fn start(self: &Arc<Self>, action: Action) {
        if self.session.lock().is_some() {
            return;
        }
        self.stop_mic_test();
        let settings = self.settings.read().clone();
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let hud = self.handle.clone();
        let level = Arc::new(move |l: f32| {
            let _ = hud.emit_to("hud", "level", l);
        });
        log::info!("session start: {action:?} (mic permission: {})", platform::permissions().microphone);
        // Non-blocking: the HUD must appear the instant Fn goes down.
        let app_err = self.clone();
        self.recorder.start(
            settings.microphone.clone(),
            level,
            Box::new(move |e| {
                log::error!("recorder start failed: {e}");
                if app_err.session.lock().as_ref().map(|s| s.id) == Some(id) {
                    // Runs on the recorder thread: don't call recorder.stop() here (it would wait on itself).
                    app_err.session.lock().take();
                    app_err.engine.lock().reset();
                    let s = app_err.settings.read().clone();
                    app_err.show_message(&format!("{}: {e}", tr(&s, "Can't open the microphone", "마이크를 열 수 없어요")), "error", 3000);
                }
            }),
        );
        // Open TLS connections / start Claude Code while the user is still talking.
        self.warm_up();
        let target_pid = Arc::new(AtomicI32::new(0));
        let capture = spawn_capture(action == Action::Ask, target_pid.clone());
        *self.session.lock() = Some(Session {
            id,
            action,
            target: settings.active_target(),
            started: Instant::now(),
            started_ms: now_ms(),
            hands_free: false,
            capture: Some(capture),
            target_pid,
        });

        self.emit_recording_state();
        self.show_hud();
        if settings.interaction_sounds {
            platform::play_sound(true);
        }
        let app = self.clone();
        std::thread::spawn(move || {
            if settings.mute_when_dictating && platform::mute_output() {
                app.muted.store(true, Ordering::SeqCst);
            }
            // Watchdog for the 9-minute cap.
            loop {
                std::thread::sleep(Duration::from_millis(500));
                let elapsed = match app.session.lock().as_ref() {
                    Some(s) if s.id == id => s.started.elapsed(),
                    _ => break,
                };
                if elapsed >= MAX_RECORDING {
                    app.engine.lock().reset();
                    app.finish();
                    break;
                }
                if elapsed >= WARN_AT {
                    let _ = app.handle.emit_to("hud", "countdown", (MAX_RECORDING - elapsed).as_secs());
                }
            }
        });
    }

    fn switch(self: &Arc<Self>, action: Action) {
        let mut guard = self.session.lock();
        let Some(s) = guard.as_mut() else { return };
        log::info!("session switch: {action:?}");
        s.action = action;
        if action == Action::Ask {
            // Re-capture including the selection (⌘C fallback) now that we know it's Ask.
            s.capture = Some(spawn_capture(true, s.target_pid.clone()));
        }
        drop(guard);
        self.emit_recording_state();
    }

    pub fn finish(self: &Arc<Self>) {
        let Some(mut session) = self.session.lock().take() else { return };
        let rec = self.recorder.stop();
        self.engine.lock().reset();
        let settings = self.settings.read().clone();
        if self.muted.swap(false, Ordering::SeqCst) {
            platform::unmute_output();
        }
        let duration = session.started.elapsed();
        self.recorder.prepare(settings.microphone.clone());
        let Some(mut rec) = rec else {
            log::warn!("finish: no recording");
            self.hide_hud();
            return;
        };
        // Dev hook: replace the microphone audio with a file (`open --env SORI_DEBUG_WAV=… Sori.app`).
        if let Some((samples, rate)) = std::env::var("SORI_DEBUG_WAV").ok().and_then(|p| std::fs::read(p).ok()).and_then(|b| audio::decode_wav(&b)) {
            rec = Recording { samples, rate };
        }
        let peak = rec.samples.iter().fold(0f32, |m, s| m.max(s.abs()));
        log::info!("session finish: {:?} {:.1}s, {} samples @{}Hz, peak {:.3}", session.action, duration.as_secs_f32(), rec.samples.len(), rec.rate, peak);
        if duration < MIN_RECORDING {
            self.hide_hud();
            return;
        }
        if settings.interaction_sounds {
            platform::play_sound(false);
        }
        if audio::is_silent(&rec.samples) {
            let msg = if platform::permissions().microphone != "granted" {
                tr(&settings, "No microphone access — allow Sori in System Settings", "마이크 권한이 없어 소리가 들어오지 않아요")
            } else {
                tr(&settings, "No speech detected", "음성이 감지되지 않았어요")
            };
            self.show_message(msg, "info", 2000);
            return;
        }
        self.emit_phase("transcribing", session.action);
        self.show_hud();
        let captured = session.capture.take();
        let app = self.clone();
        tauri::async_runtime::spawn(async move {
            let captured = tauri::async_runtime::spawn_blocking(move || captured.and_then(|h| h.join().ok()).unwrap_or_default())
                .await
                .unwrap_or_default();
            app.last_target_pid.store(captured.pid, Ordering::SeqCst);
            app.process(session, rec, captured, duration).await;
        });
    }

    pub fn cancel(self: &Arc<Self>) {
        let Some(_s) = self.session.lock().take() else { return };
        log::info!("session cancel");
        let _ = self.recorder.stop();
        self.engine.lock().reset();
        if self.muted.swap(false, Ordering::SeqCst) {
            platform::unmute_output();
        }
        self.hide_hud();
        self.recorder.prepare(self.settings.read().microphone.clone());
    }

    /// HUD buttons / language chip clicked: give focus back to the target app.
    pub fn refocus_target(&self) {
        let pid = self
            .session
            .lock()
            .as_ref()
            .map(|s| s.target_pid.load(Ordering::SeqCst))
            .unwrap_or_else(|| self.last_target_pid.load(Ordering::SeqCst));
        platform::reactivate(pid);
    }

    pub fn set_translation_target(self: &Arc<Self>, idx: usize) {
        {
            let mut st = self.settings.write();
            if idx < st.translation_targets.len() {
                st.active_translation_target = idx;
            }
        }
        let _ = self.save_settings_file();
        let target = self.settings.read().active_target();
        if let Some(s) = self.session.lock().as_mut() {
            s.target = target;
        }
        self.emit_recording_state();
        self.refocus_target();
    }

    // ------------------------------------------------------------------ processing

    async fn process(self: Arc<Self>, session: Session, rec: Recording, captured: Captured, duration: Duration) {
        let settings = self.settings.read().clone();
        let mut samples = audio::resample_to_16k(&rec.samples, rec.rate);
        audio::normalize(&mut samples);
        let wav = audio::encode_wav(&samples, audio::TARGET_RATE);
        let entry_id = uuid::Uuid::new_v4().to_string();
        let keep_history = settings.retention_ms() != Some(0);
        let audio_path = if keep_history {
            let p = self.data_dir.join("audio").join(format!("{entry_id}.wav"));
            std::fs::write(&p, &wav).ok().map(|_| p.to_string_lossy().to_string())
        } else {
            None
        };
        let mode = match session.action {
            Action::Dictate => Mode::Dictate,
            Action::Translate => Mode::Translate { target: session.target.clone() },
            Action::Ask => Mode::Ask,
        };
        let ctx = to_context(&captured);
        let dict = self.store.lock().dictionary_terms().unwrap_or_default();
        let result = self.run_pipeline(&settings, &dict, wav, &mode, &ctx, session.action).await;
        match &result {
            Ok(o) => log::info!("pipeline ok: stt {}ms, llm {}ms, action {:?}, {} chars", o.stt_ms, o.llm_ms, o.action, o.output.chars().count()),
            Err(e) => log::error!("pipeline failed: {e:#}"),
        }

        let mut entry = HistoryEntry {
            id: entry_id,
            created_at: session.started_ms,
            mode: mode.key().into(),
            app_name: captured.app_name.clone(),
            bundle_id: captured.bundle_id.clone(),
            audio_path,
            duration_ms: duration.as_millis() as i64,
            context_json: json!({"context": ctx, "mode": mode, "pid": captured.pid}).to_string(),
            ..Default::default()
        };

        match result {
            Ok(o) if o.raw.trim().is_empty() && o.output.trim().is_empty() => {
                self.show_message(tr(&settings, "No speech detected", "음성이 감지되지 않았어요"), "info", 1600);
                if let Some(p) = &entry.audio_path {
                    let _ = std::fs::remove_file(p);
                }
                return;
            }
            Ok(o) if o.output.trim().is_empty() => {
                entry.raw_text = o.raw;
                entry.status = "empty".into();
                self.show_message(tr(&settings, "Nothing to insert", "정리할 내용이 없었어요"), "info", 1600);
            }
            Ok(o) => {
                entry.raw_text = o.raw.clone();
                entry.output = o.output.clone();
                entry.language = o.language.clone();
                entry.stt_ms = o.stt_ms as i64;
                entry.llm_ms = o.llm_ms as i64;
                entry.words = text::count_words(if mode == Mode::Ask { &o.raw } else { &o.output }) as i64;
                entry.status = "ok".into();
                if o.raw.is_empty() && o.stt_ms == 0 {
                    entry.language = "one-step".into();
                }
                entry.error = o.fallback_reason.clone();
                let app = self.clone();
                let mode2 = mode.clone();
                let captured2 = captured.clone();
                let delivered = tauri::async_runtime::spawn_blocking(move || app.deliver(&mode2, &o, &captured2))
                    .await
                    .unwrap_or_else(|_| "insert".into());
                entry.action = delivered;
                let _ = self.store.lock().record_session(entry.created_at, entry.words, entry.duration_ms);
            }
            Err(e) => {
                entry.status = "error".into();
                entry.error = Some(e.to_string());
                log::error!("pipeline error: {e:#}");
                let head = tr(&settings, "Failed — you can retry from History", "처리 실패 — 기록에서 재시도할 수 있어요");
                self.show_message(&format!("{head}\n{}", short(&e.to_string(), 90)), "error", 4000);
            }
        }
        if keep_history {
            let _ = self.store.lock().upsert_history(&entry);
        }
        self.prune_history();
        let _ = self.handle.emit("history-updated", ());
    }

    async fn run_pipeline(
        &self,
        settings: &Settings,
        dict: &[String],
        wav: Vec<u8>,
        mode: &Mode,
        ctx: &Context,
        action: Action,
    ) -> anyhow::Result<pipeline::Outcome> {
        // Experimental one-step: audio straight into a multimodal LLM (not for Ask).
        if settings.one_step() && action != Action::Ask {
            self.emit_phase("listening", action);
            match pipeline::run_one_step(&self.http, settings, dict, &wav, mode, ctx).await {
                Ok(o) => return Ok(o),
                Err(e) => {
                    log::warn!("one-step failed, falling back to STT → LLM: {e:#}");
                    self.emit_phase("transcribing", action);
                }
            }
        }
        let (stt, stt_ms, stt_note) = self.transcribe(settings, dict, wav).await?;
        if stt.text.trim().is_empty() {
            return Ok(pipeline::Outcome {
                raw: String::new(),
                output: String::new(),
                action: AskAction::Insert,
                url: None,
                language: stt.language_code,
                stt_ms,
                llm_ms: 0,
                fallback_reason: stt_note,
            });
        }
        self.emit_phase("thinking", action);
        let llm = self.llm(settings);
        let mut p = pipeline::process_text(llm.as_ref(), settings, dict, &stt.text, mode, ctx).await?;
        if p.fallback_reason.is_none() {
            p.fallback_reason = stt_note;
        }
        Ok(pipeline::Outcome {
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

    fn emit_phase(&self, phase: &str, action: Action) {
        self.emit_hud(HudState { phase: phase.into(), mode: action_key(action).into(), ..Default::default() });
    }

    /// Paste / show card / open URL. Returns the delivery kind stored in history.
    fn deliver(self: &Arc<Self>, mode: &Mode, o: &pipeline::Outcome, cap: &Captured) -> String {
        let s = self.settings.read().clone();
        let keep = s.copy_to_clipboard;
        let paste = |text: &str| -> String {
            match platform::paste_text(text, cap.pid, keep) {
                Ok(()) => {
                    self.hide_hud();
                    if let Some(reason) = &o.fallback_reason {
                        self.show_message(&short(&note_text(&s, reason), 110), "info", 3000);
                    }
                    "insert".into()
                }
                Err(e) => {
                    self.show_message(&format!("{}: {e}", tr(&s, "Paste failed", "붙여넣기 실패")), "error", 3000);
                    "error".into()
                }
            }
        };
        match mode {
            Mode::Dictate | Mode::Translate { .. } => {
                if cap.field_focused == Some(false) {
                    if keep {
                        let _ = platform::copy_to_clipboard(&o.output);
                    }
                    self.hide_hud();
                    self.show_card(CardContent {
                        kind: "result".into(),
                        title: if matches!(mode, Mode::Dictate) { tr(&s, "Dictation", "받아쓰기").into() } else { tr(&s, "Translation", "번역").into() },
                        text: o.output.clone(),
                        question: String::new(),
                        copied: keep,
                    });
                    "card".into()
                } else {
                    paste(&o.output)
                }
            }
            Mode::Ask => match o.action {
                AskAction::Replace => {
                    paste(&o.output);
                    "replace".into()
                }
                AskAction::Insert => paste(&o.output),
                AskAction::OpenUrl => {
                    if let Some(url) = &o.url {
                        use tauri_plugin_opener::OpenerExt;
                        let _ = self.handle.opener().open_url(url, None::<&str>);
                    }
                    self.show_message(&short(&o.output, 80), "success", 2200);
                    "open_url".into()
                }
                AskAction::Answer => {
                    if keep {
                        let _ = platform::copy_to_clipboard(&o.output);
                    }
                    self.hide_hud();
                    self.show_card(CardContent {
                        kind: "answer".into(),
                        title: tr(&s, "Ask anything", "무엇이든 물어보세요").into(),
                        text: o.output.clone(),
                        question: o.raw.clone(),
                        copied: keep,
                    });
                    "answer".into()
                }
            },
        }
    }

    /// Re-run a history entry from its saved audio (no paste; updates history).
    pub async fn retry(self: &Arc<Self>, id: &str) -> anyhow::Result<HistoryEntry> {
        let mut e = self.store.lock().get_history(id)?.ok_or_else(|| anyhow::anyhow!("History entry not found"))?;
        let path = e.audio_path.clone().ok_or_else(|| anyhow::anyhow!("This entry has no saved audio"))?;
        let wav = std::fs::read(&path)?;
        let v: serde_json::Value = serde_json::from_str(&e.context_json).unwrap_or_default();
        let ctx: Context = serde_json::from_value(v["context"].clone()).unwrap_or_default();
        let mode: Mode = serde_json::from_value(v["mode"].clone()).unwrap_or(Mode::Dictate);
        let settings = self.settings.read().clone();
        let dict = self.store.lock().dictionary_terms().unwrap_or_default();
        let action = match &mode {
            Mode::Dictate => Action::Dictate,
            Mode::Translate { .. } => Action::Translate,
            Mode::Ask => Action::Ask,
        };
        match self.run_pipeline(&settings, &dict, wav, &mode, &ctx, action).await {
            Ok(o) => {
                let was_error = e.status != "ok";
                let one_step = o.raw.is_empty() && o.stt_ms == 0 && !o.output.is_empty();
                e.raw_text = o.raw;
                e.output = o.output;
                e.language = if one_step { "one-step".into() } else { o.language };
                e.stt_ms = o.stt_ms as i64;
                e.llm_ms = o.llm_ms as i64;
                e.words = text::count_words(if mode == Mode::Ask { &e.raw_text } else { &e.output }) as i64;
                e.status = if e.output.is_empty() { "empty".into() } else { "ok".into() };
                e.error = o.fallback_reason;
                if was_error && e.status == "ok" {
                    let _ = self.store.lock().record_session(e.created_at, e.words, e.duration_ms);
                }
            }
            Err(err) => {
                e.status = "error".into();
                e.error = Some(err.to_string());
            }
        }
        self.store.lock().upsert_history(&e)?;
        let _ = self.handle.emit("history-updated", ());
        Ok(e)
    }

    pub fn prune_history(&self) {
        let Some(ms) = self.settings.read().retention_ms() else { return };
        let cutoff = if ms == 0 { i64::MAX } else { now_ms() - ms };
        if let Ok(paths) = self.store.lock().prune_history(cutoff) {
            for p in paths {
                let _ = std::fs::remove_file(p);
            }
        }
    }

    // ------------------------------------------------------------------ mic test

    pub fn start_mic_test(self: &Arc<Self>, device: Option<String>) -> anyhow::Result<()> {
        if self.session.lock().is_some() {
            return Ok(());
        }
        let h = self.handle.clone();
        self.recorder.start(
            device,
            Arc::new(move |l| {
                let _ = h.emit_to("main", "mic-level", l);
            }),
            Box::new(|e| log::error!("mic test failed: {e}")),
        );
        self.mic_test.store(true, Ordering::SeqCst);
        Ok(())
    }

    pub fn stop_mic_test(&self) {
        if self.mic_test.swap(false, Ordering::SeqCst) {
            let _ = self.recorder.stop();
        }
    }

    // ------------------------------------------------------------------ HUD / card

    fn emit_recording_state(&self) {
        let st = {
            let guard = self.session.lock();
            let Some(s) = guard.as_ref() else { return };
            let settings = self.settings.read();
            HudState {
                phase: "recording".into(),
                mode: action_key(s.action).into(),
                hands_free: s.hands_free,
                started_at: s.started_ms,
                target: Some(s.target.clone()),
                targets: settings.translation_targets.clone(),
                target_idx: settings.active_translation_target,
                tone: "info".into(),
                ..Default::default()
            }
        };
        self.emit_hud(st);
    }

    fn emit_hud(&self, st: HudState) {
        if let Some(w) = self.handle.get_webview_window("hud") {
            platform::set_click_through(&w, !(st.phase == "recording" && st.hands_free));
        }
        let _ = self.handle.emit_to("hud", "hud-state", st);
    }

    fn show_message(self: &Arc<Self>, msg: &str, tone: &str, ms: u64) {
        self.emit_hud(HudState { phase: "message".into(), message: Some(msg.into()), tone: tone.into(), ..Default::default() });
        self.show_hud();
        let app = self.clone();
        let id_at = self.next_id.load(Ordering::SeqCst);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(ms));
            // Don't hide if a new session started meanwhile.
            if app.session.lock().is_none() && app.next_id.load(Ordering::SeqCst) == id_at {
                app.hide_hud();
            }
        });
    }

    fn show_hud(&self) {
        let Some(w) = self.handle.get_webview_window("hud") else { return };
        log::info!("hud show");
        if let Some(pos) = self.overlay_position(HUD_W, HUD_H, 14.0) {
            let _ = w.set_position(pos);
        }
        platform::show_overlay(&w);
    }

    pub fn hide_hud(&self) {
        if let Some(w) = self.handle.get_webview_window("hud") {
            self.emit_hud(HudState { phase: "hidden".into(), ..Default::default() });
            platform::hide_overlay(&w);
        }
    }

    fn show_card(&self, content: CardContent) {
        let Some(w) = self.handle.get_webview_window("card") else { return };
        let _ = self.handle.emit_to("card", "card-content", content);
        if let Some(pos) = self.overlay_position(520.0, 380.0, 120.0) {
            let _ = w.set_position(pos);
        }
        platform::show_overlay(&w);
    }

    pub fn hide_card(&self) {
        if let Some(w) = self.handle.get_webview_window("card") {
            platform::hide_overlay(&w);
        }
        platform::reactivate(self.last_target_pid.load(Ordering::SeqCst));
    }

    /// Bottom-center of the monitor under the mouse, `bottom` logical px above the work area.
    fn overlay_position(&self, w: f64, h: f64, bottom: f64) -> Option<PhysicalPosition<i32>> {
        let cursor = self.handle.cursor_position().ok()?;
        let monitors = self.handle.available_monitors().ok()?;
        let m = monitors
            .iter()
            .find(|m| {
                let p = m.position();
                let s = m.size();
                cursor.x >= p.x as f64
                    && cursor.x < (p.x + s.width as i32) as f64
                    && cursor.y >= p.y as f64
                    && cursor.y < (p.y + s.height as i32) as f64
            })
            .or_else(|| monitors.first())?;
        let scale = m.scale_factor();
        let area = m.work_area();
        let x = area.position.x as f64 + (area.size.width as f64 - w * scale) / 2.0;
        let y = area.position.y as f64 + area.size.height as f64 - (h + bottom) * scale;
        Some(PhysicalPosition::new(x.round() as i32, y.round() as i32))
    }
}

fn spawn_capture(want_selection: bool, pid_out: Arc<AtomicI32>) -> JoinHandle<Captured> {
    std::thread::spawn(move || {
        let c = platform::capture_context(want_selection);
        pid_out.store(c.pid, Ordering::SeqCst);
        c
    })
}

fn to_context(c: &Captured) -> Context {
    Context {
        app_name: c.app_name.clone(),
        bundle_id: c.bundle_id.clone(),
        window_title: c.window_title.clone(),
        field_focused: c.field_focused,
        selected_text: c.selected_text.clone(),
        selection_editable: c.selection_editable,
    }
}

fn action_key(a: Action) -> &'static str {
    match a {
        Action::Dictate => "dictate",
        Action::Translate => "translate",
        Action::Ask => "ask",
    }
}

fn short(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        s.chars().take(n).collect::<String>() + "…"
    }
}
