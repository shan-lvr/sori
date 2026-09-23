//! macOS virtual keycodes ↔ DOM-style key names.

use super::ffi::*;

/// Key-event code of the 🌐/fn key on newer Apple keyboards.
pub const GLOBE_KEYCODE: i64 = 179;

pub fn name_for_keycode(code: i64) -> String {
    let n = match code {
        0 => "KeyA", 1 => "KeyS", 2 => "KeyD", 3 => "KeyF", 4 => "KeyH", 5 => "KeyG", 6 => "KeyZ", 7 => "KeyX",
        8 => "KeyC", 9 => "KeyV", 11 => "KeyB", 12 => "KeyQ", 13 => "KeyW", 14 => "KeyE", 15 => "KeyR",
        16 => "KeyY", 17 => "KeyT", 18 => "Digit1", 19 => "Digit2", 20 => "Digit3", 21 => "Digit4", 22 => "Digit6",
        23 => "Digit5", 24 => "Equal", 25 => "Digit9", 26 => "Digit7", 27 => "Minus", 28 => "Digit8", 29 => "Digit0",
        30 => "BracketRight", 31 => "KeyO", 32 => "KeyU", 33 => "BracketLeft", 34 => "KeyI", 35 => "KeyP",
        36 => "Enter", 37 => "KeyL", 38 => "KeyJ", 39 => "Quote", 40 => "KeyK", 41 => "Semicolon", 42 => "Backslash",
        43 => "Comma", 44 => "Slash", 45 => "KeyN", 46 => "KeyM", 47 => "Period", 48 => "Tab", 49 => "Space",
        50 => "Backquote", 51 => "Backspace", 53 => "Escape", 54 => "MetaRight", 55 => "MetaLeft", 56 => "ShiftLeft",
        57 => "CapsLock", 58 => "AltLeft", 59 => "ControlLeft", 60 => "ShiftRight", 61 => "AltRight",
        62 => "ControlRight", 63 => "Fn", 64 => "F17", 79 => "F18", 80 => "F19", 90 => "F20", 96 => "F5",
        97 => "F6", 98 => "F7", 99 => "F3", 100 => "F8", 101 => "F9", 102 => "Lang2", 103 => "F11", 104 => "Lang1",
        105 => "F13", 106 => "F16", 107 => "F14", 109 => "F10", 111 => "F12", 113 => "F15", 114 => "Insert",
        115 => "Home", 116 => "PageUp", 117 => "Delete", 118 => "F4", 119 => "End", 120 => "F2", 121 => "PageDown",
        122 => "F1", 123 => "ArrowLeft", 124 => "ArrowRight", 125 => "ArrowDown", 126 => "ArrowUp",
        _ => return format!("Code{code}"),
    };
    n.to_string()
}

/// For a `flagsChanged` event: which modifier key changed and whether it is now down.
pub fn modifier_transition(code: i64, flags: u64) -> Option<(&'static str, bool)> {
    let (name, mask) = match code {
        56 => ("ShiftLeft", NX_DEVICELSHIFTKEYMASK),
        60 => ("ShiftRight", NX_DEVICERSHIFTKEYMASK),
        59 => ("ControlLeft", NX_DEVICELCTLKEYMASK),
        62 => ("ControlRight", NX_DEVICERCTLKEYMASK),
        58 => ("AltLeft", NX_DEVICELALTKEYMASK),
        61 => ("AltRight", NX_DEVICERALTKEYMASK),
        55 => ("MetaLeft", NX_DEVICELCMDKEYMASK),
        54 => ("MetaRight", NX_DEVICERCMDKEYMASK),
        63 | 179 => ("Fn", kCGEventFlagMaskSecondaryFn),
        _ => return None,
    };
    Some((name, flags & mask != 0))
}

pub fn mouse_button_name(button: i64) -> String {
    match button {
        2 => "MouseMiddle".into(),
        3 => "Mouse4".into(),
        4 => "Mouse5".into(),
        n => format!("Mouse{}", n + 1),
    }
}
