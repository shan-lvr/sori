//! macOS implementation of the platform layer.

mod appkit;
mod ax;
mod ffi;
mod input;
mod keys;
mod tap;

use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use sori_core::hotkey::Engine;
use tauri::{AppHandle, Manager, WebviewWindow};

use super::{Captured, HotkeyEvent, Permissions};

pub fn start_hotkeys(engine: Arc<Mutex<Engine>>, tx: std::sync::mpsc::Sender<HotkeyEvent>) {
    tap::spawn(engine, tx);
}

/// Capture target app + focus info. Called right when recording starts.
pub fn capture_context(want_selection: bool) -> Captured {
    let front = appkit::frontmost_app();
    let mut c = Captured {
        pid: front.pid,
        app_name: front.name,
        bundle_id: front.bundle_id,
        ..Default::default()
    };
    if front.pid == appkit::own_pid() || front.pid == 0 {
        return c;
    }
    ax::enable_manual_accessibility(front.pid);
    let f = ax::read_focus(front.pid);
    c.window_title = f.window_title;
    c.field_focused = f.editable;
    c.selected_text = f.selected_text;
    // Replace-in-place only when AX positively reports a text input; otherwise Ask answers in a card.
    c.selection_editable = f.editable == Some(true) && !is_terminal(&c.bundle_id);
    if is_terminal(&c.bundle_id) {
        // Terminals: always paste, never replace-by-selection (the selection is just highlight).
        c.field_focused = None;
    }
    if want_selection && c.selected_text.is_none() {
        c.selected_text = copy_selection();
    }
    c
}

fn is_terminal(bundle_id: &str) -> bool {
    let b = bundle_id.to_lowercase();
    ["com.apple.terminal", "iterm", "ghostty", "warp", "kitty", "alacritty", "wezterm", "hyper", "tabby"]
        .iter()
        .any(|t| b.contains(t))
}

/// Fallback for apps that don't expose AXSelectedText: synthesize ⌘C and read the
/// clipboard, then restore the previous clipboard text.
fn copy_selection() -> Option<String> {
    let mut cb = arboard::Clipboard::new().ok()?;
    let before_text = cb.get_text().ok();
    let before = appkit::pasteboard_change_count();
    input::wait_modifiers_released(Duration::from_millis(400));
    input::copy();
    let t = Instant::now();
    while t.elapsed() < Duration::from_millis(250) {
        if appkit::pasteboard_change_count() != before {
            std::thread::sleep(Duration::from_millis(20));
            let got = cb.get_text().ok().filter(|s| !s.trim().is_empty());
            if let Some(prev) = before_text {
                let _ = cb.set_text(prev);
            }
            return got;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    None
}

/// Paste `text` into `target_pid`'s focused field via the clipboard.
/// `keep_on_clipboard = false` puts the previous clipboard contents back afterwards.
pub fn paste_text(text: &str, target_pid: i32, keep_on_clipboard: bool) -> anyhow::Result<()> {
    let previous = (!keep_on_clipboard).then(appkit::pasteboard_snapshot);
    let ours = appkit::pasteboard_write_text(text, !keep_on_clipboard);
    input::wait_modifiers_released(Duration::from_millis(1500));
    if target_pid > 0 && (appkit::is_self_active() || appkit::frontmost_app().pid != target_pid) {
        appkit::activate_pid(target_pid);
        std::thread::sleep(Duration::from_millis(180));
    }
    // Give the pasteboard a beat to settle.
    std::thread::sleep(Duration::from_millis(40));
    input::paste();
    if let Some(prev) = previous {
        // The target app reads the pasteboard asynchronously; restore once it's had time,
        // and only if nobody copied something else in the meantime.
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(700));
            if appkit::pasteboard_change_count() == ours {
                appkit::pasteboard_restore(&prev);
            }
        });
    }
    Ok(())
}

/// Opaque full copy of the clipboard (all items and types).
pub struct ClipboardSnapshot(appkit::PasteboardSnapshot);

pub fn clipboard_snapshot() -> ClipboardSnapshot {
    ClipboardSnapshot(appkit::pasteboard_snapshot())
}

pub fn clipboard_restore(s: &ClipboardSnapshot) {
    appkit::pasteboard_restore(&s.0);
}

pub fn copy_to_clipboard(text: &str) -> anyhow::Result<()> {
    arboard::Clipboard::new()?.set_text(text.to_string())?;
    Ok(())
}

pub fn reactivate(pid: i32) {
    if pid > 0 && appkit::is_self_active() {
        appkit::activate_pid(pid);
    }
}

pub fn play_sound(start: bool) {
    appkit::play_sound(if start { "Tink" } else { "Pop" });
}

pub fn mute_output() -> bool {
    appkit::mute_output()
}

pub fn unmute_output() {
    appkit::unmute_output()
}

pub fn permissions() -> Permissions {
    Permissions {
        accessibility: ax::is_trusted(),
        microphone: match appkit::microphone_status() {
            3 => "granted",
            2 | 1 => "denied",
            _ => "unknown",
        }
        .into(),
        fn_usage_type: appkit::fn_usage_type(),
    }
}

pub fn request_accessibility() {
    ax::prompt_trust();
}

pub fn open_privacy_pane(kind: &str) {
    let anchor = match kind {
        "microphone" => "Privacy_Microphone",
        "input" => "Privacy_ListenEvent",
        "keyboard" => "",
        _ => "Privacy_Accessibility",
    };
    let url = if kind == "keyboard" {
        "x-apple.systempreferences:com.apple.Keyboard-Settings.extension".to_string()
    } else {
        format!("x-apple.systempreferences:com.apple.preference.security?{anchor}")
    };
    let _ = std::process::Command::new("open").arg(url).status();
}

pub fn fn_usage_type() -> Option<i32> {
    appkit::fn_usage_type()
}

pub fn set_fn_usage_type(v: i32) -> bool {
    appkit::set_fn_usage_type(v)
}

pub fn configure_overlay(w: &WebviewWindow) {
    let w2 = w.clone();
    let _ = w.run_on_main_thread(move || {
        if let Ok(ns) = w2.ns_window() {
            unsafe { appkit::configure_overlay(ns) };
        }
    });
}

pub fn set_click_through(w: &WebviewWindow, through: bool) {
    let w2 = w.clone();
    let _ = w.run_on_main_thread(move || {
        if let Ok(ns) = w2.ns_window() {
            unsafe { appkit::set_ignores_mouse(ns, through) };
        }
    });
}

pub fn show_overlay(w: &WebviewWindow) {
    let w2 = w.clone();
    let _ = w.run_on_main_thread(move || {
        if let Ok(ns) = w2.ns_window() {
            unsafe { appkit::order_front(ns) };
        }
    });
}

pub fn hide_overlay(w: &WebviewWindow) {
    let w2 = w.clone();
    let _ = w.run_on_main_thread(move || {
        if let Ok(ns) = w2.ns_window() {
            unsafe { appkit::order_out(ns) };
        }
    });
}

pub fn set_dock_visible(app: &AppHandle, visible: bool) {
    let policy = if visible { tauri::ActivationPolicy::Regular } else { tauri::ActivationPolicy::Accessory };
    let _ = app.set_activation_policy(policy);
    // Switching to Accessory hides windows; keep the main window reachable.
    if let Some(w) = app.get_webview_window("main") {
        if w.is_visible().unwrap_or(false) {
            let _ = w.show();
        }
    }
}
