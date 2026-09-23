//! Placeholder platform layer for Windows/Linux so the crate compiles everywhere.
//! Clipboard works; global hotkeys, paste and context capture are TODO.

use std::sync::Arc;

use parking_lot::Mutex;
use sori_core::hotkey::Engine;
use tauri::{AppHandle, WebviewWindow};

use super::{Captured, HotkeyEvent, Permissions};

pub fn start_hotkeys(_engine: Arc<Mutex<Engine>>, tx: std::sync::mpsc::Sender<HotkeyEvent>) {
    let _ = tx.send(HotkeyEvent::TapUnavailable);
}

pub fn capture_context(_want_selection: bool) -> Captured {
    Captured::default()
}

pub fn paste_text(text: &str, _target_pid: i32, _keep_on_clipboard: bool) -> anyhow::Result<()> {
    copy_to_clipboard(text)
}

pub struct ClipboardSnapshot(Option<String>);

pub fn clipboard_snapshot() -> ClipboardSnapshot {
    ClipboardSnapshot(arboard::Clipboard::new().ok().and_then(|mut c| c.get_text().ok()))
}

pub fn clipboard_restore(s: &ClipboardSnapshot) {
    if let (Some(t), Ok(mut c)) = (&s.0, arboard::Clipboard::new()) {
        let _ = c.set_text(t.clone());
    }
}

pub fn copy_to_clipboard(text: &str) -> anyhow::Result<()> {
    arboard::Clipboard::new()?.set_text(text.to_string())?;
    Ok(())
}

pub fn reactivate(_pid: i32) {}
pub fn play_sound(_start: bool) {}
pub fn mute_output() -> bool {
    false
}
pub fn unmute_output() {}

pub fn permissions() -> Permissions {
    Permissions { accessibility: true, microphone: "unknown".into(), fn_usage_type: None }
}
pub fn request_accessibility() {}
pub fn open_privacy_pane(_kind: &str) {}
pub fn fn_usage_type() -> Option<i32> {
    None
}
pub fn set_fn_usage_type(_v: i32) -> bool {
    false
}
pub fn configure_overlay(_w: &WebviewWindow) {}
pub fn set_click_through(_w: &WebviewWindow, _through: bool) {}
pub fn show_overlay(w: &WebviewWindow) {
    let _ = w.show();
}
pub fn hide_overlay(w: &WebviewWindow) {
    let _ = w.hide();
}
pub fn set_dock_visible(_app: &AppHandle, _visible: bool) {}
