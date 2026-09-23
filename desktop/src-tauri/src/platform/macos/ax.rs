//! Accessibility (AX) helpers: focused element, selected text, window title.

use core_foundation::base::TCFType;
use core_foundation::string::CFString;

use super::ffi::*;

/// Owned AX/CF reference released on drop.
struct Owned(CFTypeRef);
impl Drop for Owned {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { CFRelease(self.0) };
        }
    }
}

fn attr(el: AXUIElementRef, name: &str) -> Option<Owned> {
    let key = CFString::new(name);
    let mut out: CFTypeRef = std::ptr::null();
    let err = unsafe { AXUIElementCopyAttributeValue(el, key.as_concrete_TypeRef() as CFStringRef, &mut out) };
    if err == kAXErrorSuccess && !out.is_null() {
        Some(Owned(out))
    } else {
        None
    }
}

fn attr_string(el: AXUIElementRef, name: &str) -> Option<String> {
    let v = attr(el, name)?;
    unsafe {
        if CFGetTypeID(v.0) != CFStringGetTypeID() {
            return None;
        }
        // Retain for the wrapper (we still release `v` on drop).
        let s = CFString::wrap_under_get_rule(v.0 as _);
        Some(s.to_string())
    }
}

fn settable(el: AXUIElementRef, name: &str) -> bool {
    let key = CFString::new(name);
    let mut b: u8 = 0;
    let err = unsafe { AXUIElementIsAttributeSettable(el, key.as_concrete_TypeRef() as CFStringRef, &mut b) };
    err == kAXErrorSuccess && b != 0
}

pub fn is_trusted() -> bool {
    unsafe { AXIsProcessTrusted() }
}

pub fn prompt_trust() -> bool {
    use core_foundation::boolean::CFBoolean;
    use core_foundation::dictionary::CFDictionary;
    unsafe {
        let key = CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt as _);
        let dict = CFDictionary::from_CFType_pairs(&[(key, CFBoolean::true_value())]);
        AXIsProcessTrustedWithOptions(dict.as_concrete_TypeRef() as _)
    }
}

/// Ask Chromium/Electron apps to expose their accessibility tree.
pub fn enable_manual_accessibility(pid: i32) {
    unsafe {
        let app = AXUIElementCreateApplication(pid);
        if app.is_null() {
            return;
        }
        let _g = Owned(app);
        let key = CFString::new("AXManualAccessibility");
        AXUIElementSetAttributeValue(app, key.as_concrete_TypeRef() as CFStringRef, kCFBooleanTrue);
    }
}

#[derive(Debug, Default, Clone)]
pub struct Focus {
    /// None if no focused element could be read.
    pub editable: Option<bool>,
    pub selected_text: Option<String>,
    pub window_title: String,
}

const TEXT_ROLES: &[&str] = &["AXTextField", "AXTextArea", "AXComboBox", "AXSearchField"];
/// Roles that clearly cannot take typed text. Anything else (AXGroup, AXWebArea, unknown…)
/// is treated as "maybe editable" so we still paste — Electron apps and terminals often
/// report generic roles for their text inputs.
const NON_TEXT_ROLES: &[&str] = &[
    "AXButton", "AXList", "AXOutline", "AXTable", "AXImage", "AXMenuItem", "AXMenu", "AXMenuBar",
    "AXCheckBox", "AXRadioButton", "AXPopUpButton", "AXSlider", "AXStaticText", "AXRow", "AXCell",
    "AXWindow", "AXToolbar", "AXTabGroup", "AXBrowser", "AXDockItem", "AXLink",
];

pub fn read_focus(pid: i32) -> Focus {
    let mut f = Focus::default();
    unsafe {
        let app = AXUIElementCreateApplication(pid);
        if app.is_null() {
            return f;
        }
        let app = Owned(app);
        AXUIElementSetMessagingTimeout(app.0, 0.3);
        if let Some(win) = attr(app.0, "AXFocusedWindow") {
            f.window_title = attr_string(win.0, "AXTitle").unwrap_or_default();
        }
        let focused = attr(app.0, "AXFocusedUIElement").or_else(|| {
            let sys = AXUIElementCreateSystemWide();
            if sys.is_null() {
                return None;
            }
            let sys = Owned(sys);
            AXUIElementSetMessagingTimeout(sys.0, 0.3);
            attr(sys.0, "AXFocusedUIElement")
        });
        if let Some(el) = focused {
            let role = attr_string(el.0, "AXRole").unwrap_or_default();
            f.editable = if TEXT_ROLES.contains(&role.as_str()) || settable(el.0, "AXValue") {
                Some(true)
            } else if NON_TEXT_ROLES.contains(&role.as_str()) {
                Some(false)
            } else {
                None
            };
            f.selected_text = attr_string(el.0, "AXSelectedText").filter(|s| !s.is_empty());
        }
    }
    f
}
