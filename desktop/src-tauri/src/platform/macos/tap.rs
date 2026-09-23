//! Global keyboard / mouse-button hook via a CGEventTap on its own run-loop thread.
#![allow(non_upper_case_globals)]

use std::ffi::c_void;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use sori_core::hotkey::{Engine, Output};

use super::ffi::*;
use super::keys;
use crate::platform::HotkeyEvent;

/// Marker placed in `kCGEventSourceUserData` on events we synthesize so the tap ignores them.
pub const SYNTH_MARKER: i64 = 0x534F_5249; // "SORI"

struct TapCtx {
    engine: Arc<Mutex<Engine>>,
    tx: std::sync::mpsc::Sender<HotkeyEvent>,
    port: AtomicPtr<c_void>,
}

extern "C" fn callback(_proxy: CGEventTapProxy, etype: u32, event: CGEventRef, user_info: *mut c_void) -> CGEventRef {
    let ctx = unsafe { &*(user_info as *const TapCtx) };
    if etype == kCGEventTapDisabledByTimeout || etype == kCGEventTapDisabledByUserInput {
        let port = ctx.port.load(Ordering::SeqCst);
        if !port.is_null() {
            unsafe { CGEventTapEnable(port, true) };
        }
        return event;
    }
    if event.is_null() {
        return event;
    }
    if unsafe { CGEventGetIntegerValueField(event, kCGEventSourceUserData) } == SYNTH_MARKER {
        return event;
    }

    let (key, down): (String, bool) = unsafe {
        match etype {
            kCGEventFlagsChanged => {
                let code = CGEventGetIntegerValueField(event, kCGKeyboardEventKeycode);
                match keys::modifier_transition(code, CGEventGetFlags(event)) {
                    Some((name, down)) => (name.to_string(), down),
                    None => return event,
                }
            }
            kCGEventKeyDown | kCGEventKeyUp => {
                let code = CGEventGetIntegerValueField(event, kCGKeyboardEventKeycode);
                if code == keys::GLOBE_KEYCODE {
                    // The 🌐 key also sends its own key events; Fn state comes from flagsChanged
                    // only (feeding both double-counts presses). Just hide them from the system.
                    return if ctx.engine.lock().uses_key("Fn") { std::ptr::null_mut() } else { event };
                }
                (keys::name_for_keycode(code), etype == kCGEventKeyDown)
            }
            kCGEventOtherMouseDown | kCGEventOtherMouseUp => {
                let b = CGEventGetIntegerValueField(event, kCGMouseEventButtonNumber);
                (keys::mouse_button_name(b), etype == kCGEventOtherMouseDown)
            }
            _ => return event,
        }
    };

    let (decision, recorded, swallow_fn) = {
        let mut eng = ctx.engine.lock();
        let d = eng.on_key(&key, down, Instant::now());
        // Like Typeless: when Fn is one of our shortcuts, eat the Fn events at the HID level so
        // macOS doesn't also run its 🌐 action (input-source switch / emoji picker / dictation).
        let swallow_fn = key == "Fn" && eng.uses_key("Fn");
        (d, eng.take_recorded(), swallow_fn)
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
    if decision.swallow || swallow_fn {
        std::ptr::null_mut()
    } else {
        event
    }
}

/// Spawn the tap thread. Retries until Accessibility permission is granted.
pub fn spawn(engine: Arc<Mutex<Engine>>, tx: std::sync::mpsc::Sender<HotkeyEvent>) {
    std::thread::Builder::new()
        .name("sori-event-tap".into())
        .spawn(move || {
            let ctx = Box::into_raw(Box::new(TapCtx { engine, tx, port: AtomicPtr::new(std::ptr::null_mut()) }));
            let mask: u64 = (1 << kCGEventKeyDown)
                | (1 << kCGEventKeyUp)
                | (1 << kCGEventFlagsChanged)
                | (1 << kCGEventOtherMouseDown)
                | (1 << kCGEventOtherMouseUp);
            let mut warned = false;
            loop {
                let port = unsafe {
                    CGEventTapCreate(
                        // HID level: we see Fn before the system turns it into a 🌐 action.
                        kCGHIDEventTap,
                        kCGHeadInsertEventTap,
                        kCGEventTapOptionDefault,
                        mask,
                        callback,
                        ctx as *mut c_void,
                    )
                };
                if port.is_null() {
                    if !warned {
                        log::warn!("event tap unavailable (Accessibility not granted?) — retrying");
                        let _ = unsafe { &*ctx }.tx.send(HotkeyEvent::TapUnavailable);
                        warned = true;
                    }
                    std::thread::sleep(Duration::from_secs(2));
                    continue;
                }
                unsafe {
                    (*ctx).port.store(port, Ordering::SeqCst);
                    let src = CFMachPortCreateRunLoopSource(std::ptr::null(), port, 0);
                    CFRunLoopAddSource(CFRunLoopGetCurrent(), src, kCFRunLoopCommonModes);
                    CGEventTapEnable(port, true);
                    let _ = (*ctx).tx.send(HotkeyEvent::TapReady);
                    log::info!("event tap running");
                    CFRunLoopRun();
                }
                // Run loop exited unexpectedly; recreate.
                std::thread::sleep(Duration::from_secs(1));
            }
        })
        .expect("spawn tap thread");
}
