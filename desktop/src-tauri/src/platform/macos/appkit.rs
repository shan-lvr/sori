//! AppKit / misc system helpers via the Objective-C runtime.

use std::ffi::{c_void, CString};

use objc2::rc::{autoreleasepool, Retained};
use objc2::runtime::{AnyClass, AnyObject};
use objc2::{class, msg_send};
use objc2_foundation::{NSArray, NSData, NSString};

#[link(name = "AVFoundation", kind = "framework")]
extern "C" {}

#[derive(Debug, Clone, Default)]
pub struct FrontApp {
    pub pid: i32,
    pub name: String,
    pub bundle_id: String,
}

unsafe fn ns_to_string(p: *mut NSString) -> String {
    if p.is_null() {
        String::new()
    } else {
        (*p).to_string()
    }
}

pub fn frontmost_app() -> FrontApp {
    autoreleasepool(|_| unsafe {
        let ws: *mut AnyObject = msg_send![class!(NSWorkspace), sharedWorkspace];
        let app: *mut AnyObject = msg_send![ws, frontmostApplication];
        if app.is_null() {
            return FrontApp::default();
        }
        let name: *mut NSString = msg_send![app, localizedName];
        let bid: *mut NSString = msg_send![app, bundleIdentifier];
        let pid: i32 = msg_send![app, processIdentifier];
        FrontApp { pid, name: ns_to_string(name), bundle_id: ns_to_string(bid) }
    })
}

pub fn own_pid() -> i32 {
    std::process::id() as i32
}

/// Bring another app back to the front (after a click on our HUD/card took focus).
pub fn activate_pid(pid: i32) {
    autoreleasepool(|_| unsafe {
        let app: *mut AnyObject = msg_send![class!(NSRunningApplication), runningApplicationWithProcessIdentifier: pid];
        if !app.is_null() {
            let _: bool = msg_send![app, activateWithOptions: 2usize];
        }
    })
}

pub fn is_self_active() -> bool {
    autoreleasepool(|_| unsafe {
        let cur: *mut AnyObject = msg_send![class!(NSRunningApplication), currentApplication];
        let active: bool = msg_send![cur, isActive];
        active
    })
}

pub fn pasteboard_change_count() -> isize {
    autoreleasepool(|_| unsafe {
        let pb: *mut AnyObject = msg_send![class!(NSPasteboard), generalPasteboard];
        let c: isize = msg_send![pb, changeCount];
        c
    })
}

/// Full copy of the general pasteboard (every item, every type) so it can be put back.
pub struct PasteboardSnapshot(Vec<Vec<(String, Vec<u8>)>>);

pub fn pasteboard_snapshot() -> PasteboardSnapshot {
    autoreleasepool(|_| unsafe {
        let pb: *mut AnyObject = msg_send![class!(NSPasteboard), generalPasteboard];
        let items: *mut AnyObject = msg_send![pb, pasteboardItems];
        let mut out = vec![];
        if items.is_null() {
            return PasteboardSnapshot(out);
        }
        let n: usize = msg_send![items, count];
        for i in 0..n {
            let item: *mut AnyObject = msg_send![items, objectAtIndex: i];
            let types: *mut AnyObject = msg_send![item, types];
            if types.is_null() {
                continue;
            }
            let tn: usize = msg_send![types, count];
            let mut entries = vec![];
            for j in 0..tn {
                let ty: *mut NSString = msg_send![types, objectAtIndex: j];
                let data: *mut NSData = msg_send![item, dataForType: ty];
                if ty.is_null() || data.is_null() {
                    continue;
                }
                let ptr: *const u8 = msg_send![data, bytes];
                let len: usize = msg_send![data, length];
                let bytes = if ptr.is_null() || len == 0 { vec![] } else { std::slice::from_raw_parts(ptr, len).to_vec() };
                entries.push(((*ty).to_string(), bytes));
            }
            out.push(entries);
        }
        PasteboardSnapshot(out)
    })
}

pub fn pasteboard_restore(snap: &PasteboardSnapshot) {
    autoreleasepool(|_| unsafe {
        let pb: *mut AnyObject = msg_send![class!(NSPasteboard), generalPasteboard];
        let _: isize = msg_send![pb, clearContents];
        if snap.0.is_empty() {
            return;
        }
        let mut items: Vec<Retained<AnyObject>> = vec![];
        for entries in &snap.0 {
            let item: Retained<AnyObject> = msg_send![class!(NSPasteboardItem), new];
            for (ty, bytes) in entries {
                let data = NSData::with_bytes(bytes);
                let ty = NSString::from_str(ty);
                let _: bool = msg_send![&*item, setData: &*data, forType: &*ty];
            }
            items.push(item);
        }
        let arr = NSArray::from_retained_slice(&items);
        let _: bool = msg_send![pb, writeObjects: &*arr];
    })
}

/// Put plain text on the general pasteboard. `transient` marks it (nspasteboard.org
/// convention) so clipboard managers don't record it. Returns the new change count.
pub fn pasteboard_write_text(text: &str, transient: bool) -> isize {
    autoreleasepool(|_| unsafe {
        let pb: *mut AnyObject = msg_send![class!(NSPasteboard), generalPasteboard];
        let _: isize = msg_send![pb, clearContents];
        let item: Retained<AnyObject> = msg_send![class!(NSPasteboardItem), new];
        let s = NSString::from_str(text);
        let ty = NSString::from_str("public.utf8-plain-text");
        let _: bool = msg_send![&*item, setString: &*s, forType: &*ty];
        if transient {
            let empty = NSData::new();
            for marker in ["org.nspasteboard.TransientType", "org.nspasteboard.AutoGeneratedType"] {
                let ty = NSString::from_str(marker);
                let _: bool = msg_send![&*item, setData: &*empty, forType: &*ty];
            }
        }
        let arr = NSArray::from_retained_slice(&[item]);
        let _: bool = msg_send![pb, writeObjects: &*arr];
        let c: isize = msg_send![pb, changeCount];
        c
    })
}

pub fn play_sound(name: &str) {
    let name = name.to_string();
    autoreleasepool(|_| unsafe {
        let ns = NSString::from_str(&name);
        let s: *mut AnyObject = msg_send![class!(NSSound), soundNamed: &*ns];
        if !s.is_null() {
            let _: () = msg_send![s, setVolume: 0.35f32];
            let _: bool = msg_send![s, play];
        }
    })
}

/// 0 not determined, 1 restricted, 2 denied, 3 authorized.
pub fn microphone_status() -> i64 {
    autoreleasepool(|_| unsafe {
        let Some(cls) = AnyClass::get(c"AVCaptureDevice") else { return 0 };
        let media = NSString::from_str("soun");
        let s: isize = msg_send![cls, authorizationStatusForMediaType: &*media];
        s as i64
    })
}

/// Make an overlay window float above everything (incl. full-screen apps) on every Space
/// without ever becoming key. Must run on the main thread.
pub unsafe fn configure_overlay(ns_window: *mut c_void) {
    let w = ns_window as *mut AnyObject;
    if w.is_null() {
        return;
    }
    // canJoinAllSpaces | stationary | ignoresCycle | fullScreenAuxiliary
    let behavior: usize = (1 << 0) | (1 << 4) | (1 << 6) | (1 << 8);
    let _: () = msg_send![w, setCollectionBehavior: behavior];
    let _: () = msg_send![w, setLevel: 101isize]; // NSPopUpMenuWindowLevel
    let _: () = msg_send![w, setHidesOnDeactivate: false];
    let _: () = msg_send![w, setHasShadow: false];
}

/// Show without activating the app or making the window key. Main thread only.
pub unsafe fn order_front(ns_window: *mut c_void) {
    let w = ns_window as *mut AnyObject;
    if !w.is_null() {
        let _: () = msg_send![w, orderFrontRegardless];
    }
}

pub unsafe fn set_ignores_mouse(ns_window: *mut c_void, ignore: bool) {
    let w = ns_window as *mut AnyObject;
    if !w.is_null() {
        let _: () = msg_send![w, setIgnoresMouseEvents: ignore];
    }
}

pub unsafe fn order_out(ns_window: *mut c_void) {
    let w = ns_window as *mut AnyObject;
    if !w.is_null() {
        let _: () = msg_send![w, orderOut: std::ptr::null_mut::<AnyObject>()];
    }
}

// ---- "Press 🌐 key to" (AppleFnUsageType) via private HIToolbox calls ----
// 0 = Do Nothing, 1 = Change Input Source, 2 = Show Emoji & Symbols, 3 = Start Dictation.

type GetFn = unsafe extern "C" fn() -> i32;
type SetFn = unsafe extern "C" fn(i32);

fn carbon_sym(name: &str) -> *mut c_void {
    unsafe {
        let lib = CString::new("/System/Library/Frameworks/Carbon.framework/Carbon").unwrap();
        let h = libc::dlopen(lib.as_ptr(), libc::RTLD_LAZY);
        if h.is_null() {
            return std::ptr::null_mut();
        }
        let n = CString::new(name).unwrap();
        libc::dlsym(h, n.as_ptr())
    }
}

pub fn fn_usage_type() -> Option<i32> {
    let p = carbon_sym("TISGetFnUsageType");
    if p.is_null() {
        return None;
    }
    let f: GetFn = unsafe { std::mem::transmute(p) };
    Some(unsafe { f() })
}

pub fn set_fn_usage_type(v: i32) -> bool {
    let p = carbon_sym("TISUpdateFnUsageType");
    if p.is_null() {
        return false;
    }
    let f: SetFn = unsafe { std::mem::transmute(p) };
    unsafe { f(v) };
    true
}

// ---- system output mute (for "Mute when dictating") ----

/// Returns true if we muted (i.e. output was not already muted).
pub fn mute_output() -> bool {
    let out = std::process::Command::new("osascript")
        .args(["-e", "output muted of (get volume settings)"])
        .output();
    let already = out.map(|o| String::from_utf8_lossy(&o.stdout).trim() == "true").unwrap_or(true);
    if already {
        return false;
    }
    let _ = std::process::Command::new("osascript").args(["-e", "set volume output muted true"]).status();
    true
}

pub fn unmute_output() {
    let _ = std::process::Command::new("osascript").args(["-e", "set volume output muted false"]).status();
}
