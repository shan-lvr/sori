//! Raw CoreGraphics / ApplicationServices / Carbon declarations we need.
#![allow(non_upper_case_globals, non_snake_case, dead_code)]

use std::ffi::c_void;

pub type CFTypeRef = *const c_void;
pub type CFStringRef = *const c_void;
pub type CFDictionaryRef = *const c_void;
pub type CFMachPortRef = *mut c_void;
pub type CFRunLoopSourceRef = *mut c_void;
pub type CFRunLoopRef = *mut c_void;
pub type CGEventRef = *mut c_void;
pub type CGEventSourceRef = *mut c_void;
pub type CGEventTapProxy = *mut c_void;
pub type AXUIElementRef = *const c_void;
pub type AXError = i32;

pub type CGEventTapCallBack =
    extern "C" fn(proxy: CGEventTapProxy, etype: u32, event: CGEventRef, user_info: *mut c_void) -> CGEventRef;

// Event tap locations / placement / options
pub const kCGHIDEventTap: u32 = 0;
pub const kCGSessionEventTap: u32 = 1;
pub const kCGHeadInsertEventTap: u32 = 0;
pub const kCGEventTapOptionDefault: u32 = 0;

// Event types
pub const kCGEventKeyDown: u32 = 10;
pub const kCGEventKeyUp: u32 = 11;
pub const kCGEventFlagsChanged: u32 = 12;
pub const kCGEventOtherMouseDown: u32 = 25;
pub const kCGEventOtherMouseUp: u32 = 26;
pub const kCGEventTapDisabledByTimeout: u32 = 0xFFFF_FFFE;
pub const kCGEventTapDisabledByUserInput: u32 = 0xFFFF_FFFF;

// Event fields
pub const kCGMouseEventButtonNumber: u32 = 3;
pub const kCGKeyboardEventAutorepeat: u32 = 8;
pub const kCGKeyboardEventKeycode: u32 = 9;
pub const kCGEventSourceUserData: u32 = 42;

// Flags
pub const kCGEventFlagMaskAlphaShift: u64 = 0x0001_0000;
pub const kCGEventFlagMaskShift: u64 = 0x0002_0000;
pub const kCGEventFlagMaskControl: u64 = 0x0004_0000;
pub const kCGEventFlagMaskAlternate: u64 = 0x0008_0000;
pub const kCGEventFlagMaskCommand: u64 = 0x0010_0000;
pub const kCGEventFlagMaskSecondaryFn: u64 = 0x0080_0000;
pub const NX_DEVICELCTLKEYMASK: u64 = 0x0000_0001;
pub const NX_DEVICELSHIFTKEYMASK: u64 = 0x0000_0002;
pub const NX_DEVICERSHIFTKEYMASK: u64 = 0x0000_0004;
pub const NX_DEVICELCMDKEYMASK: u64 = 0x0000_0008;
pub const NX_DEVICERCMDKEYMASK: u64 = 0x0000_0010;
pub const NX_DEVICELALTKEYMASK: u64 = 0x0000_0020;
pub const NX_DEVICERALTKEYMASK: u64 = 0x0000_0040;
pub const NX_DEVICERCTLKEYMASK: u64 = 0x0000_2000;

// Event source states
pub const kCGEventSourceStatePrivate: i32 = -1;
pub const kCGEventSourceStateCombinedSessionState: i32 = 0;
pub const kCGEventSourceStateHIDSystemState: i32 = 1;

pub const kAXErrorSuccess: AXError = 0;

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    pub fn CGEventTapCreate(
        tap: u32,
        place: u32,
        options: u32,
        events_of_interest: u64,
        callback: CGEventTapCallBack,
        user_info: *mut c_void,
    ) -> CFMachPortRef;
    pub fn CGEventTapEnable(tap: CFMachPortRef, enable: bool);
    pub fn CGEventGetIntegerValueField(event: CGEventRef, field: u32) -> i64;
    pub fn CGEventSetIntegerValueField(event: CGEventRef, field: u32, value: i64);
    pub fn CGEventGetFlags(event: CGEventRef) -> u64;
    pub fn CGEventSetFlags(event: CGEventRef, flags: u64);
    pub fn CGEventSourceCreate(state: i32) -> CGEventSourceRef;
    pub fn CGEventCreateKeyboardEvent(source: CGEventSourceRef, keycode: u16, keydown: bool) -> CGEventRef;
    pub fn CGEventPost(tap: u32, event: CGEventRef);
    pub fn CGEventSourceFlagsState(state: i32) -> u64;
    pub fn CGPreflightListenEventAccess() -> bool;
    pub fn CGRequestListenEventAccess() -> bool;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    pub fn CFMachPortCreateRunLoopSource(alloc: *const c_void, port: CFMachPortRef, order: isize) -> CFRunLoopSourceRef;
    pub fn CFRunLoopGetCurrent() -> CFRunLoopRef;
    pub fn CFRunLoopAddSource(rl: CFRunLoopRef, source: CFRunLoopSourceRef, mode: CFStringRef);
    pub fn CFRunLoopRun();
    pub fn CFRelease(cf: CFTypeRef);
    pub fn CFGetTypeID(cf: CFTypeRef) -> usize;
    pub fn CFStringGetTypeID() -> usize;
    pub static kCFRunLoopCommonModes: CFStringRef;
    pub static kCFBooleanTrue: CFTypeRef;
}

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    pub fn AXIsProcessTrusted() -> bool;
    pub fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> bool;
    pub static kAXTrustedCheckOptionPrompt: CFStringRef;
    pub fn AXUIElementCreateSystemWide() -> AXUIElementRef;
    pub fn AXUIElementCreateApplication(pid: i32) -> AXUIElementRef;
    pub fn AXUIElementCopyAttributeValue(el: AXUIElementRef, attr: CFStringRef, value: *mut CFTypeRef) -> AXError;
    pub fn AXUIElementSetAttributeValue(el: AXUIElementRef, attr: CFStringRef, value: CFTypeRef) -> AXError;
    pub fn AXUIElementIsAttributeSettable(el: AXUIElementRef, attr: CFStringRef, settable: *mut u8) -> AXError;
    pub fn AXUIElementSetMessagingTimeout(el: AXUIElementRef, timeout: f32) -> AXError;
}
