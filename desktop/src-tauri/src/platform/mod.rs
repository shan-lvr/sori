//! OS integration layer. Each platform module exposes the same free functions:
//! hotkeys, context capture, paste, overlays, permissions, sounds.

use serde::Serialize;
use sori_core::hotkey::Action;

#[derive(Debug, Clone)]
pub enum HotkeyEvent {
    Start(Action),
    Switch(Action),
    HandsFree,
    Stop,
    Cancel,
    /// A shortcut captured in "record shortcut" mode.
    Recorded(Vec<String>),
    TapReady,
    TapUnavailable,
}

/// What the platform could learn about the target app at recording start.
#[derive(Debug, Clone, Default)]
pub struct Captured {
    pub pid: i32,
    pub app_name: String,
    pub bundle_id: String,
    pub window_title: String,
    pub field_focused: Option<bool>,
    pub selected_text: Option<String>,
    pub selection_editable: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Permissions {
    pub accessibility: bool,
    /// `granted` | `denied` | `unknown`
    pub microphone: String,
    /// macOS "Press 🌐 key to": 0 nothing, 1 input source, 2 emoji, 3 dictation.
    pub fn_usage_type: Option<i32>,
}

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::*;

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use windows::*;

// Linux: placeholder (clipboard only) so the crate builds everywhere.
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod fallback;
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub use fallback::*;
