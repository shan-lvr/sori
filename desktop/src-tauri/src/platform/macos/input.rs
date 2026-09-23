//! Synthesized keystrokes (⌘V / ⌘C) and modifier-state polling.

use std::time::{Duration, Instant};

use super::ffi::*;
use super::tap::SYNTH_MARKER;

const KEY_CMD: u16 = 55;
/// `v` / `c` on ANSI — also correct with the Korean 2-Set input source active (⌘ resolves
/// through the underlying `2SetHangul` layout, which maps these to the same keycodes).
const KEY_V: u16 = 9;
const KEY_C: u16 = 8;

fn post_cmd_combo(key: u16) {
    unsafe {
        let src = CGEventSourceCreate(kCGEventSourceStatePrivate);
        let seq = [(KEY_CMD, true), (key, true), (key, false), (KEY_CMD, false)];
        for (code, down) in seq {
            let ev = CGEventCreateKeyboardEvent(src, code, down);
            if ev.is_null() {
                continue;
            }
            let flags = if code == KEY_CMD && !down { 0 } else { kCGEventFlagMaskCommand };
            CGEventSetFlags(ev, flags);
            CGEventSetIntegerValueField(ev, kCGEventSourceUserData, SYNTH_MARKER);
            CGEventPost(kCGHIDEventTap, ev);
            CFRelease(ev as _);
            std::thread::sleep(Duration::from_millis(8));
        }
        if !src.is_null() {
            CFRelease(src as _);
        }
    }
}

pub fn paste() {
    post_cmd_combo(KEY_V);
}

pub fn copy() {
    post_cmd_combo(KEY_C);
}

/// Wait (up to `max`) until the user has physically released Shift/Ctrl/Option/⌘/Fn so a
/// synthesized ⌘V isn't turned into ⇧⌘V etc.
pub fn wait_modifiers_released(max: Duration) {
    let mask = kCGEventFlagMaskShift
        | kCGEventFlagMaskControl
        | kCGEventFlagMaskAlternate
        | kCGEventFlagMaskCommand
        | kCGEventFlagMaskSecondaryFn;
    let t = Instant::now();
    while t.elapsed() < max {
        let flags = unsafe { CGEventSourceFlagsState(kCGEventSourceStateHIDSystemState) };
        if flags & mask == 0 {
            return;
        }
        std::thread::sleep(Duration::from_millis(15));
    }
}
