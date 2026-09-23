//! Windows implementation of the platform layer.
//!
//! - Shortcuts: `WH_KEYBOARD_LL` / `WH_MOUSE_LL` hooks on their own message-loop thread,
//!   fed into the same `Engine` as macOS (key names are DOM-style: "ControlLeft", "MetaLeft"…).
//! - Paste: clipboard + `SendInput(Ctrl+V)`; our own synthetic events carry a marker in
//!   `dwExtraInfo` so the hook ignores them.
//! - Context: foreground window → process exe name (`bundle_id = "win:<exe>"`) + title; a
//!   blinking caret (`GetGUIThreadInfo`) means a text field is focused.
#![allow(clippy::missing_safety_doc)]

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use sori_core::hotkey::{Engine, Output};
use tauri::{AppHandle, Manager, WebviewWindow};
use windows_sys::Win32::Foundation::{CloseHandle, HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::DataExchange::GetClipboardSequenceNumber;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use super::{Captured, HotkeyEvent, Permissions};

/// `dwExtraInfo` on events we synthesize ("SORI").
const SYNTH_MARKER: usize = 0x534F_5249;
/// Unassigned virtual key used to stop Windows from opening the Start menu when a shortcut
/// that includes the Win key is released (same trick as AutoHotkey's MenuMaskKey).
const VK_MASK: u16 = 0xE8;

struct HookCtx {
    engine: Arc<Mutex<Engine>>,
    tx: std::sync::mpsc::Sender<HotkeyEvent>,
    /// A Win key took part in a shortcut during the current press.
    win_used: AtomicBool,
}

static CTX: OnceLock<HookCtx> = OnceLock::new();
static KB_HOOK: AtomicIsize = AtomicIsize::new(0);
static MOUSE_HOOK: AtomicIsize = AtomicIsize::new(0);
/// Foreground window captured at recording start (for re-activation before pasting).
static LAST_HWND: AtomicIsize = AtomicIsize::new(0);

fn key_name(vk: u32) -> String {
    let n = match vk {
        0x08 => "Backspace",
        0x09 => "Tab",
        0x0D => "Enter",
        0x13 => "Pause",
        0x14 => "CapsLock",
        0x15 => "Lang1", // 한/영 (Korean layouts map Right Alt here)
        0x19 => "Lang2", // 한자
        0x1B => "Escape",
        0x20 => "Space",
        0x21 => "PageUp",
        0x22 => "PageDown",
        0x23 => "End",
        0x24 => "Home",
        0x25 => "ArrowLeft",
        0x26 => "ArrowUp",
        0x27 => "ArrowRight",
        0x28 => "ArrowDown",
        0x2D => "Insert",
        0x2E => "Delete",
        0x5B => "MetaLeft",
        0x5C => "MetaRight",
        0x5D => "ContextMenu",
        0xA0 => "ShiftLeft",
        0xA1 => "ShiftRight",
        0xA2 => "ControlLeft",
        0xA3 => "ControlRight",
        0xA4 => "AltLeft",
        0xA5 => "AltRight",
        0xBA => "Semicolon",
        0xBB => "Equal",
        0xBC => "Comma",
        0xBD => "Minus",
        0xBE => "Period",
        0xBF => "Slash",
        0xC0 => "Backquote",
        0xDB => "BracketLeft",
        0xDC => "Backslash",
        0xDD => "BracketRight",
        0xDE => "Quote",
        0x30..=0x39 => return format!("Digit{}", vk - 0x30),
        0x41..=0x5A => return format!("Key{}", char::from_u32(vk).unwrap_or('?')),
        0x70..=0x87 => return format!("F{}", vk - 0x6F),
        _ => return format!("Code{vk}"),
    };
    n.to_string()
}

fn feed(ctx: &HookCtx, key: &str, down: bool) -> bool {
    let (decision, recorded) = {
        let mut eng = ctx.engine.lock();
        let d = eng.on_key(key, down, Instant::now());
        (d, eng.take_recorded())
    };
    for o in decision.outputs {
        let ev = match o {
            Output::Start(a) => HotkeyEvent::Start(a),
            Output::Switch(a) => HotkeyEvent::Switch(a),
            Output::HandsFree => HotkeyEvent::HandsFree,
            Output::Stop => HotkeyEvent::Stop,
            Output::Cancel => HotkeyEvent::Cancel,
        };
        let _ = ctx.tx.send(ev);
    }
    if let Some(combo) = recorded {
        let _ = ctx.tx.send(HotkeyEvent::Recorded(combo));
    }
    decision.swallow
}

unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let hook = KB_HOOK.load(Ordering::Relaxed) as HHOOK;
    if code != HC_ACTION as i32 {
        return CallNextHookEx(hook, code, wparam, lparam);
    }
    let Some(ctx) = CTX.get() else { return CallNextHookEx(hook, code, wparam, lparam) };
    let kb = &*(lparam as *const KBDLLHOOKSTRUCT);
    if kb.dwExtraInfo == SYNTH_MARKER || kb.vkCode == VK_MASK as u32 {
        return CallNextHookEx(hook, code, wparam, lparam);
    }
    let down = matches!(wparam as u32, WM_KEYDOWN | WM_SYSKEYDOWN);
    let key = key_name(kb.vkCode);
    let is_win = kb.vkCode == 0x5B || kb.vkCode == 0x5C;
    let uses_win = is_win && ctx.engine.lock().uses_key(&key);
    let swallow = feed(ctx, &key, down);
    if uses_win && down {
        ctx.win_used.store(true, Ordering::Relaxed);
    }
    if is_win && !down && ctx.win_used.swap(false, Ordering::Relaxed) {
        // Keep the Start menu closed after a Ctrl+Win shortcut.
        send_keys(&[(VK_MASK, true), (VK_MASK, false)]);
    }
    if swallow {
        return 1;
    }
    CallNextHookEx(hook, code, wparam, lparam)
}

unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let hook = MOUSE_HOOK.load(Ordering::Relaxed) as HHOOK;
    if code != HC_ACTION as i32 {
        return CallNextHookEx(hook, code, wparam, lparam);
    }
    let Some(ctx) = CTX.get() else { return CallNextHookEx(hook, code, wparam, lparam) };
    let m = &*(lparam as *const MSLLHOOKSTRUCT);
    let (key, down) = match wparam as u32 {
        WM_MBUTTONDOWN => ("MouseMiddle", true),
        WM_MBUTTONUP => ("MouseMiddle", false),
        WM_XBUTTONDOWN | WM_XBUTTONUP => {
            let b = (m.mouseData >> 16) & 0xFFFF;
            (if b == 1 { "Mouse4" } else { "Mouse5" }, wparam as u32 == WM_XBUTTONDOWN)
        }
        _ => return CallNextHookEx(hook, code, wparam, lparam),
    };
    if feed(ctx, key, down) {
        return 1;
    }
    CallNextHookEx(hook, code, wparam, lparam)
}

pub fn start_hotkeys(engine: Arc<Mutex<Engine>>, tx: std::sync::mpsc::Sender<HotkeyEvent>) {
    let _ = CTX.set(HookCtx { engine, tx: tx.clone(), win_used: AtomicBool::new(false) });
    std::thread::Builder::new()
        .name("sori-hooks".into())
        .spawn(move || unsafe {
            let hmod = GetModuleHandleW(std::ptr::null());
            let kb = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), hmod, 0);
            let ms = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), hmod, 0);
            if kb.is_null() {
                log::error!("keyboard hook failed");
                let _ = tx.send(HotkeyEvent::TapUnavailable);
                return;
            }
            KB_HOOK.store(kb as isize, Ordering::SeqCst);
            MOUSE_HOOK.store(ms as isize, Ordering::SeqCst);
            let _ = tx.send(HotkeyEvent::TapReady);
            log::info!("keyboard hook running");
            // LL hooks are called on this thread; it must pump messages.
            let mut msg: MSG = std::mem::zeroed();
            while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        })
        .expect("spawn hook thread");
}

// ------------------------------------------------------------------ synthetic input

fn send_keys(seq: &[(u16, bool)]) {
    let inputs: Vec<INPUT> = seq
        .iter()
        .map(|&(vk, down)| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT { wVk: vk, wScan: 0, dwFlags: if down { 0 } else { KEYEVENTF_KEYUP }, time: 0, dwExtraInfo: SYNTH_MARKER },
            },
        })
        .collect();
    unsafe { SendInput(inputs.len() as u32, inputs.as_ptr(), std::mem::size_of::<INPUT>() as i32) };
}

const VK_CONTROL: u16 = 0x11;

fn ctrl_combo(key: u16) {
    send_keys(&[(VK_CONTROL, true), (key, true), (key, false), (VK_CONTROL, false)]);
}

/// Wait until Shift/Ctrl/Alt/Win are physically released so Ctrl+V isn't Ctrl+Shift+V etc.
fn wait_modifiers_released(max: Duration) {
    let t = Instant::now();
    while t.elapsed() < max {
        let held = [0x10, 0x11, 0x12, 0x5B, 0x5C].iter().any(|&vk| unsafe { GetAsyncKeyState(vk) } as u16 & 0x8000 != 0);
        if !held {
            return;
        }
        std::thread::sleep(Duration::from_millis(15));
    }
}

// ------------------------------------------------------------------ context

fn exe_name(pid: u32) -> String {
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            return String::new();
        }
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len);
        CloseHandle(h);
        if ok == 0 {
            return String::new();
        }
        let path = String::from_utf16_lossy(&buf[..len as usize]);
        path.rsplit(['\\', '/']).next().unwrap_or("").to_string()
    }
}

fn window_title(hwnd: HWND) -> String {
    let mut buf = [0u16; 512];
    let n = unsafe { GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

/// Friendly app name from an exe: "Code.exe" → "Code", "ms-teams.exe" → "ms-teams".
fn display_name(exe: &str) -> String {
    exe.strip_suffix(".exe").or_else(|| exe.strip_suffix(".EXE")).unwrap_or(exe).to_string()
}

pub fn capture_context(want_selection: bool) -> Captured {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_null() {
            return Captured::default();
        }
        let mut pid = 0u32;
        let thread = GetWindowThreadProcessId(hwnd, &mut pid);
        if pid == std::process::id() {
            return Captured { pid: pid as i32, app_name: "Sori".into(), ..Default::default() };
        }
        LAST_HWND.store(hwnd as isize, Ordering::SeqCst);
        let exe = exe_name(pid);
        let mut info: GUITHREADINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<GUITHREADINFO>() as u32;
        let caret = GetGUIThreadInfo(thread, &mut info) != 0 && !info.hwndCaret.is_null();
        let terminal = {
            let e = exe.to_lowercase();
            ["windowsterminal", "wt.exe", "powershell", "pwsh", "cmd.exe", "conhost", "mintty", "wezterm", "alacritty"].iter().any(|t| e.contains(t))
        };
        let mut c = Captured {
            pid: pid as i32,
            app_name: display_name(&exe),
            bundle_id: format!("win:{}", exe.to_lowercase()),
            window_title: window_title(hwnd),
            // No caret doesn't prove there's no field (Chromium/Electron apps draw their own),
            // so only a positive signal is trusted; unknown means "paste".
            field_focused: if caret { Some(true) } else { None },
            selected_text: None,
            selection_editable: caret && !terminal,
        };
        if terminal {
            c.field_focused = None;
        }
        if want_selection {
            c.selected_text = copy_selection();
            // Apps without a system caret (browsers, Electron) usually still accept a paste over
            // the selection when it came from an input; we can't tell, so keep the safe default.
        }
        c
    }
}

/// Ctrl+C the current selection and read it, restoring the previous clipboard text.
fn copy_selection() -> Option<String> {
    let mut cb = arboard::Clipboard::new().ok()?;
    let before_text = cb.get_text().ok();
    let before = unsafe { GetClipboardSequenceNumber() };
    wait_modifiers_released(Duration::from_millis(400));
    ctrl_combo(0x43); // C
    let t = Instant::now();
    while t.elapsed() < Duration::from_millis(300) {
        if unsafe { GetClipboardSequenceNumber() } != before {
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

// ------------------------------------------------------------------ paste / clipboard

fn activate(hwnd: HWND) {
    if hwnd.is_null() {
        return;
    }
    unsafe {
        if IsIconic(hwnd) != 0 {
            ShowWindow(hwnd, SW_RESTORE);
        }
        SetForegroundWindow(hwnd);
    }
}

fn foreground_is_self() -> bool {
    unsafe {
        let mut pid = 0u32;
        GetWindowThreadProcessId(GetForegroundWindow(), &mut pid);
        pid == std::process::id()
    }
}

pub fn paste_text(text: &str, target_pid: i32, keep_on_clipboard: bool) -> anyhow::Result<()> {
    let mut cb = arboard::Clipboard::new()?;
    let previous = if keep_on_clipboard { None } else { cb.get_text().ok() };
    cb.set_text(text.to_string())?;
    let ours = unsafe { GetClipboardSequenceNumber() };
    wait_modifiers_released(Duration::from_millis(1500));
    let hwnd = LAST_HWND.load(Ordering::SeqCst) as HWND;
    if target_pid > 0 && foreground_is_self() {
        activate(hwnd);
        std::thread::sleep(Duration::from_millis(150));
    }
    std::thread::sleep(Duration::from_millis(30));
    ctrl_combo(0x56); // V
    if let Some(prev) = previous {
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(700));
            if unsafe { GetClipboardSequenceNumber() } == ours {
                if let Ok(mut c) = arboard::Clipboard::new() {
                    let _ = c.set_text(prev);
                }
            }
        });
    }
    Ok(())
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

pub fn reactivate(pid: i32) {
    if pid > 0 && foreground_is_self() {
        activate(LAST_HWND.load(Ordering::SeqCst) as HWND);
    }
}

// ------------------------------------------------------------------ sounds / audio

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

pub fn play_sound(start: bool) {
    use windows_sys::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_FILENAME, SND_NODEFAULT};
    let windir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into());
    let file = format!("{windir}\\Media\\{}", if start { "Speech On.wav" } else { "Speech Off.wav" });
    let w = wide(&file);
    unsafe { PlaySoundW(w.as_ptr(), std::ptr::null_mut(), SND_ASYNC | SND_FILENAME | SND_NODEFAULT) };
}

pub fn mute_output() -> bool {
    false
}
pub fn unmute_output() {}

// ------------------------------------------------------------------ permissions

/// Windows' microphone privacy switch (Settings → Privacy → Microphone → desktop apps).
fn microphone_state() -> &'static str {
    use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ};
    let read = |root: HKEY, sub: &str| -> Option<String> {
        let (k, v) = (wide(sub), wide("Value"));
        let mut buf = [0u16; 32];
        let mut len = (buf.len() * 2) as u32;
        let rc = unsafe { RegGetValueW(root, k.as_ptr(), v.as_ptr(), RRF_RT_REG_SZ, std::ptr::null_mut(), buf.as_mut_ptr() as *mut c_void, &mut len) };
        (rc == 0).then(|| String::from_utf16_lossy(&buf[..(len as usize / 2).saturating_sub(1)]))
    };
    let base = "Software\\Microsoft\\Windows\\CurrentVersion\\CapabilityAccessManager\\ConsentStore\\microphone";
    let denied = |root| read(root, base).as_deref() == Some("Deny") || read(root, &format!("{base}\\NonPackaged")).as_deref() == Some("Deny");
    if denied(HKEY_LOCAL_MACHINE) || denied(HKEY_CURRENT_USER) {
        "denied"
    } else {
        "granted"
    }
}

pub fn permissions() -> Permissions {
    // No Accessibility-style permission on Windows: hooks and SendInput just work (except
    // into elevated apps, which Windows blocks for non-elevated processes).
    Permissions { accessibility: true, microphone: microphone_state().into(), fn_usage_type: None }
}

pub fn request_accessibility() {}

fn shell_open(target: &str) {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    let (op, t) = (wide("open"), wide(target));
    unsafe { ShellExecuteW(std::ptr::null_mut(), op.as_ptr(), t.as_ptr(), std::ptr::null(), std::ptr::null(), SW_SHOWNORMAL) };
}

pub fn open_privacy_pane(kind: &str) {
    if kind == "microphone" {
        shell_open("ms-settings:privacy-microphone");
    }
}

pub fn fn_usage_type() -> Option<i32> {
    None
}
pub fn set_fn_usage_type(_v: i32) -> bool {
    false
}

// ------------------------------------------------------------------ overlays

pub fn configure_overlay(w: &WebviewWindow) {
    let _ = w.set_always_on_top(true);
    let _ = w.set_skip_taskbar(true);
}

pub fn set_click_through(w: &WebviewWindow, through: bool) {
    let _ = w.set_ignore_cursor_events(through);
}

pub fn show_overlay(w: &WebviewWindow) {
    // The HUD/card windows are created non-focusable, so showing them doesn't steal focus
    // from the app the user is typing into.
    let _ = w.show();
    let _ = w.set_always_on_top(true);
}

pub fn hide_overlay(w: &WebviewWindow) {
    let _ = w.hide();
}

pub fn set_dock_visible(app: &AppHandle, visible: bool) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.set_skip_taskbar(!visible);
    }
}
