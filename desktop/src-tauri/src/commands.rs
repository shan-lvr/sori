//! Tauri commands invoked from the UI.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use serde::Serialize;
use sori_core::settings::Settings;
use sori_core::store::{DictWord, HistoryEntry, Stats};
use sori_core::{pipeline, Context, Mode};
use tauri::{AppHandle, Emitter, State};

use crate::controller::{default_settings, App};
use crate::platform::{self, Permissions};

type S<'a> = State<'a, Arc<App>>;
type R<T> = Result<T, String>;

fn e<E: std::fmt::Display>(err: E) -> String {
    err.to_string()
}

#[tauri::command]
pub fn get_settings(app: S) -> Settings {
    app.settings.read().clone()
}

#[tauri::command]
pub fn save_settings(app: S, settings: Settings) -> R<Settings> {
    let prev = app.settings.read().clone();
    *app.settings.write() = settings;
    app.save_settings_file().map_err(e)?;
    apply_settings(&app, Some(&prev));
    let s = app.settings.read().clone();
    let _ = app.handle.emit("settings-changed", &s);
    Ok(s)
}

/// Apply side effects of settings (shortcuts, autostart, dock, 🌐 key).
pub fn apply_settings(app: &Arc<App>, prev: Option<&Settings>) {
    let s = app.settings.read().clone();
    {
        let mut eng = app.engine.lock();
        eng.set_shortcuts(&s.shortcuts);
        eng.set_toggle_mode(s.recording_mode != "hybrid");
    }
    if prev.map_or(false, |p| p.microphone != s.microphone) {
        app.recorder.prepare(s.microphone.clone());
    }
    if prev.map_or(false, |p| p.stt_engine != s.stt_engine || p.local_model != s.local_model) {
        if s.stt_engine == "local" {
            app.local_stt.clear_pause(&s.local_model);
        }
        app.sync_local_stt();
    }
    if prev.map_or(false, |p| p.interface_language != s.interface_language) {
        crate::refresh_tray(&app.handle);
    }
    if prev.map_or(false, |p| p.llm_provider != s.llm_provider || p.claude_model != s.claude_model || p.claude_effort != s.claude_effort) {
        app.claude.reset_workers();
        app.warm_up();
    }

    if prev.map_or(true, |p| p.launch_at_login != s.launch_at_login) {
        use tauri_plugin_autostart::ManagerExt;
        let al = app.handle.autolaunch();
        let enabled = al.is_enabled().unwrap_or(false);
        if s.launch_at_login && !enabled {
            let _ = al.enable();
        } else if !s.launch_at_login && enabled && prev.is_some() {
            let _ = al.disable();
        }
    }
    if prev.map_or(true, |p| p.show_in_dock != s.show_in_dock) {
        platform::set_dock_visible(&app.handle, s.show_in_dock);
    }
    if s.manage_fn_key {
        let mut saved = app.saved_fn_usage.lock();
        if saved.is_none() {
            if let Some(cur) = platform::fn_usage_type() {
                if cur != 0 {
                    *saved = Some(cur);
                    platform::set_fn_usage_type(0);
                }
            }
        }
    } else {
        restore_fn_usage(app);
    }
    if prev.map_or(false, |p| p.history_retention != s.history_retention) {
        app.prune_history();
        let _ = app.handle.emit("history-updated", ());
    }
}

pub fn restore_fn_usage(app: &Arc<App>) {
    if let Some(v) = app.saved_fn_usage.lock().take() {
        platform::set_fn_usage_type(v);
    }
}

#[tauri::command]
pub fn reset_api_key(app: S, which: String) -> R<Settings> {
    let d = default_settings();
    {
        let mut s = app.settings.write();
        match which.as_str() {
            "elevenlabs" => s.elevenlabs_api_key = d.elevenlabs_api_key,
            "openrouter" => s.openrouter_api_key = d.openrouter_api_key,
            _ => return Err("unknown key".into()),
        }
    }
    app.save_settings_file().map_err(e)?;
    Ok(app.settings.read().clone())
}

#[derive(Serialize)]
pub struct KeyCheck {
    pub elevenlabs: Result<String, String>,
    pub openrouter: Result<String, String>,
}

#[tauri::command]
pub async fn test_api_keys(app: S<'_>, elevenlabs: String, openrouter: String) -> R<KeyCheck> {
    // ElevenLabs: transcribe half a second of silence (works with STT-scoped keys).
    let silence = sori_core::audio::encode_wav(&vec![0.0; 8000], 16_000);
    let el = match sori_core::stt::transcribe(&app.http, &elevenlabs, "scribe_v2", silence, None, &[]).await {
        Ok(_) => Ok("OK".to_string()),
        Err(err) => Err(err.to_string()),
    };
    let or = async {
        let r = app
            .http
            .get("https://openrouter.ai/api/v1/key")
            .bearer_auth(openrouter.trim())
            .send()
            .await
            .map_err(e)?;
        let status = r.status();
        let v: serde_json::Value = r.json().await.map_err(e)?;
        if !status.is_success() {
            return Err(format!("{}: {}", status.as_u16(), v["error"]["message"].as_str().unwrap_or("")));
        }
        let d = &v["data"];
        Ok(match (d["limit_remaining"].as_f64(), d["usage"].as_f64()) {
            (Some(rem), _) => format!("OK · ${rem:.2} left"),
            (None, Some(u)) => format!("OK · ${u:.2} used"),
            _ => "OK".into(),
        })
    }
    .await;
    Ok(KeyCheck { elevenlabs: el, openrouter: or })
}

#[derive(Serialize)]
pub struct PermState {
    #[serde(flatten)]
    pub perms: Permissions,
    pub hotkeys_ready: bool,
    pub fn_managed: bool,
    /// Typeless also listens to Fn; running both double-triggers.
    pub typeless_running: bool,
}

#[tauri::command]
pub fn get_permissions(app: S) -> PermState {
    PermState {
        perms: platform::permissions(),
        hotkeys_ready: app.tap_ready.load(Ordering::SeqCst),
        fn_managed: app.saved_fn_usage.lock().is_some(),
        typeless_running: std::process::Command::new("pgrep")
            .args(["-x", "Typeless"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false),
    }
}

#[tauri::command]
pub fn request_accessibility() {
    platform::request_accessibility();
}

#[tauri::command]
pub fn open_privacy_pane(kind: String) {
    platform::open_privacy_pane(&kind);
}

#[tauri::command]
pub fn list_microphones() -> Vec<String> {
    crate::recorder::list_devices()
}

#[tauri::command]
pub fn start_mic_test(app: S, device: Option<String>) -> R<()> {
    app.start_mic_test(device).map_err(e)
}

#[tauri::command]
pub fn stop_mic_test(app: S) {
    app.stop_mic_test();
}

#[tauri::command]
pub fn list_history(app: S, filter: String, query: String, limit: i64, offset: i64) -> R<Vec<HistoryEntry>> {
    app.store.lock().list_history(&filter, &query, limit, offset).map_err(e)
}

// Command bodies live in plain functions so the in-app self-test exercises the same code.

pub fn history_delete(app: &App, id: &str) -> anyhow::Result<()> {
    if let Some(p) = app.store.lock().delete_history(id)? {
        let _ = std::fs::remove_file(p);
    }
    Ok(())
}

pub fn history_delete_all(app: &App) -> anyhow::Result<()> {
    for p in app.store.lock().delete_all_history()? {
        let _ = std::fs::remove_file(p);
    }
    Ok(())
}

pub fn history_export_audio(app: &App, id: &str, dest: &str) -> anyhow::Result<()> {
    let entry = app.store.lock().get_history(id)?.ok_or_else(|| anyhow::anyhow!("History entry not found"))?;
    let src = entry.audio_path.ok_or_else(|| anyhow::anyhow!("This entry has no saved audio"))?;
    std::fs::copy(src, dest)?;
    Ok(())
}

pub fn dictionary_import(app: &App, path: &str) -> anyhow::Result<usize> {
    let content = std::fs::read_to_string(path)?;
    let mut terms = vec![];
    for (i, line) in content.lines().enumerate() {
        let first = line.split([',', '\t', ';']).next().unwrap_or("").trim().trim_matches('"').trim();
        if first.is_empty() {
            continue;
        }
        if i == 0 && matches!(first.to_lowercase().as_str(), "word" | "words" | "term" | "terms" | "단어" | "용어") {
            continue;
        }
        terms.push(first.to_string());
    }
    app.store.lock().add_words(&terms, "manual")
}

#[tauri::command]
pub fn delete_history(app: S, id: String) -> R<()> {
    history_delete(&app, &id).map_err(e)
}

#[tauri::command]
pub fn delete_all_history(app: S) -> R<()> {
    history_delete_all(&app).map_err(e)
}

#[tauri::command]
pub async fn retry_history(app: S<'_>, id: String) -> R<HistoryEntry> {
    let a = app.inner().clone();
    a.retry(&id).await.map_err(e)
}

#[tauri::command]
pub fn export_audio(app: S, id: String, dest: String) -> R<()> {
    history_export_audio(&app, &id, &dest).map_err(e)
}

#[tauri::command]
pub fn copy_text(text: String) -> R<()> {
    platform::copy_to_clipboard(&text).map_err(e)
}

#[tauri::command]
pub fn list_dictionary(app: S) -> R<Vec<DictWord>> {
    app.store.lock().list_dictionary().map_err(e)
}

#[tauri::command]
pub fn add_words(app: S, terms: Vec<String>) -> R<usize> {
    app.store.lock().add_words(&terms, "manual").map_err(e)
}

#[tauri::command]
pub fn update_word(app: S, id: i64, term: String) -> R<()> {
    app.store.lock().update_word(id, &term).map_err(e)
}

#[tauri::command]
pub fn delete_words(app: S, ids: Vec<i64>) -> R<()> {
    app.store.lock().delete_words(&ids).map_err(e)
}

/// Import words from a CSV/TXT file (first column; header row ignored).
#[tauri::command]
pub fn import_dictionary(app: S, path: String) -> R<usize> {
    dictionary_import(&app, &path).map_err(e)
}

#[tauri::command]
pub fn local_models(app: S) -> Vec<crate::local_stt::ModelStatus> {
    app.local_stt.status()
}

/// Start (or resume) a model download in the background; progress arrives as
/// `local-model-download` events. Returns immediately.
#[tauri::command]
pub fn download_local_model(app: S, id: String) {
    app.start_model_download(&id);
}

#[tauri::command]
pub fn pause_local_download(app: S) {
    app.local_stt.pause_download();
}

#[tauri::command]
pub fn cancel_local_download(app: S, id: String) {
    app.local_stt.cancel_download(&id);
    let _ = app.handle.emit("local-model-download", serde_json::json!({"id": id, "state": "cancelled"}));
}

#[tauri::command]
pub fn delete_local_model(app: S, id: String) -> R<()> {
    app.local_stt.delete(&id).map_err(e)
}

#[tauri::command]
pub fn get_stats(app: S) -> R<Stats> {
    let wpm = app.settings.read().typing_wpm;
    let factor = app.settings.read().compose_factor;
    app.store.lock().stats(wpm, factor).map_err(e)
}

#[tauri::command]
pub fn hud_stop(app: S) {
    app.engine.lock().reset();
    app.finish();
    app.refocus_target();
}

#[tauri::command]
pub fn hud_cancel(app: S) {
    app.cancel();
    app.refocus_target();
}

#[tauri::command]
pub fn hud_set_target(app: S, idx: usize) {
    app.set_translation_target(idx);
}

#[tauri::command]
pub fn card_close(app: S) {
    app.hide_card();
}

#[tauri::command]
pub fn record_shortcut_start(app: S) {
    app.engine.lock().begin_shortcut_recording();
}

#[tauri::command]
pub fn record_shortcut_cancel(app: S) {
    app.engine.lock().cancel_shortcut_recording();
}

#[derive(Serialize)]
pub struct AppInfo {
    pub version: String,
    pub data_dir: String,
    pub config_path: String,
    pub has_default_keys: bool,
    pub platform: String,
    pub default_shortcuts: sori_core::settings::Shortcuts,
}

#[tauri::command]
pub fn app_info(app: S, handle: AppHandle) -> AppInfo {
    let d = default_settings();
    AppInfo {
        version: handle.package_info().version.to_string(),
        data_dir: app.data_dir.to_string_lossy().to_string(),
        config_path: app.config_path.to_string_lossy().to_string(),
        has_default_keys: !d.elevenlabs_api_key.is_empty() && !d.openrouter_api_key.is_empty(),
        platform: std::env::consts::OS.into(),
        default_shortcuts: d.shortcuts,
    }
}

/// Settings → "Try the prompt": run the LLM step on typed text (no STT).
#[tauri::command]
pub async fn process_text_preview(app: S<'_>, text: String, mode: String) -> R<String> {
    let settings = app.settings.read().clone();
    let dict = app.store.lock().dictionary_terms().unwrap_or_default();
    let mode = match mode.as_str() {
        "translate" => Mode::Translate { target: settings.active_target() },
        _ => Mode::Dictate,
    };
    let ctx = Context::default();
    let llm = app.llm(&settings);
    let p = pipeline::process_text(llm.as_ref(), &settings, &dict, &text, &mode, &ctx).await.map_err(e)?;
    Ok(p.output)
}

// ---------------------------------------------------------------- Claude Code CLI

#[tauri::command]
pub async fn claude_status(app: S<'_>) -> R<crate::claude_cli::CliStatus> {
    Ok(app.claude.status().await)
}

/// Install the CLI with Anthropic's official installer; output streams as `claude-log`.
#[tauri::command]
pub async fn claude_install(app: S<'_>) -> R<crate::claude_cli::CliStatus> {
    let h = app.handle.clone();
    let _ = h.emit("claude-log", "$ curl -fsSL https://claude.ai/install.sh | bash");
    app.claude
        .install(move |line| {
            let _ = h.emit("claude-log", line);
        })
        .await
        .map_err(e)?;
    Ok(app.claude.status().await)
}

/// `claude auth login`: opens the browser; finishes by itself once the user approves.
/// Output streams as `claude-log`; the sign-in URL (fallback link) as `claude-login-url`.
#[tauri::command]
pub async fn claude_login(app: S<'_>) -> R<crate::claude_cli::CliStatus> {
    let h = app.handle.clone();
    let res = app
        .claude
        .login(move |line| {
            if let Some(url) = crate::claude_cli::find_url(&line) {
                let _ = h.emit("claude-login-url", url);
            }
            let _ = h.emit("claude-log", line);
        })
        .await;
    app.claude.reset_workers();
    let st = app.claude.status().await;
    if st.logged_in {
        app.warm_up();
    }
    match res {
        Err(err) if !st.logged_in => Err(e(err)),
        _ => Ok(st),
    }
}

#[tauri::command]
pub async fn claude_login_code(app: S<'_>, code: String) -> R<()> {
    app.claude.submit_code(&code).await.map_err(e)
}

#[tauri::command]
pub fn claude_login_cancel(app: S) {
    app.claude.cancel_login();
}

/// One real cleanup through `claude -p` to confirm everything works and show the latency.
#[tauri::command]
pub async fn claude_test(app: S<'_>) -> R<String> {
    let mut s = app.settings.read().clone();
    s.llm_provider = "claude_code".into();
    let t = std::time::Instant::now();
    let llm = crate::claude_cli::ClaudeLlm { cli: app.claude.clone(), effort: s.claude_effort.clone() };
    let p = pipeline::process_text(&llm, &s, &[], "um so let's uh meet tomorrow at three no wait four", &Mode::Dictate, &Context::default())
        .await
        .map_err(e)?;
    if let Some(r) = p.fallback_reason {
        return Err(r);
    }
    Ok(format!("{} · {:.1}s", p.output.trim(), t.elapsed().as_secs_f32()))
}

