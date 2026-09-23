//! Platform-independent shortcut state machine.
//!
//! The platform layer feeds raw key/mouse-button transitions (by name) and gets back what
//! to do (start/switch/stop/cancel a recording) and whether to swallow the event.
//!
//! Behaviour mirrors Typeless:
//! - Toggle mode (default): press a shortcut once to start, keep talking hands-free, press the
//!   main (dictate) shortcut again to finish. Esc cancels.
//! - Hybrid mode: holding records while held (push-to-talk); a short tap behaves like toggle.
//! - Recording starts on key-down of the first matching set (e.g. `Fn`) so the mic is
//!   live immediately; adding `ShiftLeft`/`Space` while `Fn` is held switches the mode.
//! - Pressing an unrelated key while the shortcut is held (Fn+←, Fn+Delete…) cancels
//!   silently and lets the key through.

use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use crate::settings::Shortcuts;

#[derive(Copy, Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    Dictate,
    Translate,
    Ask,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Output {
    Start(Action),
    Switch(Action),
    /// Recording continues after the keys were released.
    HandsFree,
    Stop,
    Cancel,
}

#[derive(Default, Debug)]
pub struct Decision {
    pub outputs: Vec<Output>,
    pub swallow: bool,
}

#[derive(Clone, Debug, PartialEq)]
enum State {
    Idle,
    /// Keys are held; recording.
    Held { action: Action, since: Instant, started: Instant },
    /// Keys released after a tap; recording until the finish shortcut.
    HandsFree { action: Action, started: Instant },
    /// A stop/cancel happened while keys were still down; wait for all keys up.
    Suppressed,
}

pub fn is_modifier(key: &str) -> bool {
    matches!(
        key,
        "Fn" | "ShiftLeft"
            | "ShiftRight"
            | "ControlLeft"
            | "ControlRight"
            | "AltLeft"
            | "AltRight"
            | "MetaLeft"
            | "MetaRight"
            | "CapsLock"
    )
}

pub struct Engine {
    bindings: Vec<(Action, BTreeSet<String>)>,
    held: BTreeSet<String>,
    swallowed: BTreeSet<String>,
    state: State,
    tap_threshold: Duration,
    /// Toggle mode: releasing the keys never stops the recording.
    toggle: bool,
    /// When set, the next complete combo is reported via `take_recorded` instead of acting.
    recording_shortcut: bool,
    recorded_peak: BTreeSet<String>,
    recorded: Option<Vec<String>>,
}

impl Engine {
    pub fn new(shortcuts: &Shortcuts) -> Self {
        let mut e = Self {
            bindings: vec![],
            held: BTreeSet::new(),
            swallowed: BTreeSet::new(),
            state: State::Idle,
            tap_threshold: Duration::from_millis(300),
            toggle: true,
            recording_shortcut: false,
            recorded_peak: BTreeSet::new(),
            recorded: None,
        };
        e.set_shortcuts(shortcuts);
        e
    }

    pub fn set_shortcuts(&mut self, s: &Shortcuts) {
        let mut b = vec![];
        for (action, list) in [(Action::Dictate, &s.dictate), (Action::Translate, &s.translate), (Action::Ask, &s.ask)] {
            for sc in list {
                if !sc.is_empty() {
                    b.push((action, sc.iter().cloned().collect()));
                }
            }
        }
        self.bindings = b;
    }

    /// `true` = press to start / press again to stop; `false` = hold to talk (tap still toggles).
    pub fn set_toggle_mode(&mut self, toggle: bool) {
        self.toggle = toggle;
    }

    /// Controller-initiated end of session (error, max length, HUD button…).
    pub fn reset(&mut self) {
        self.state = if self.held.is_empty() { State::Idle } else { State::Suppressed };
    }

    /// Whether any shortcut uses `key` (e.g. to decide if Fn events should be swallowed).
    pub fn uses_key(&self, key: &str) -> bool {
        self.recording_shortcut || self.bindings.iter().any(|(_, keys)| keys.contains(key))
    }

    pub fn is_active(&self) -> bool {
        matches!(self.state, State::Held { .. } | State::HandsFree { .. })
    }

    pub fn begin_shortcut_recording(&mut self) {
        self.recording_shortcut = true;
        self.recorded_peak.clear();
        self.recorded = None;
    }

    pub fn cancel_shortcut_recording(&mut self) {
        self.recording_shortcut = false;
        self.recorded_peak.clear();
    }

    pub fn take_recorded(&mut self) -> Option<Vec<String>> {
        self.recorded.take()
    }

    fn binding_for(&self, set: &BTreeSet<String>) -> Option<Action> {
        self.bindings.iter().find(|(_, keys)| keys == set).map(|(a, _)| *a)
    }

    fn is_prefix_of_binding(&self, set: &BTreeSet<String>) -> bool {
        self.bindings.iter().any(|(_, keys)| set.is_subset(keys))
    }

    fn is_finish_combo(&self, set: &BTreeSet<String>, current: Action) -> bool {
        self.bindings
            .iter()
            .any(|(a, keys)| keys == set && (*a == Action::Dictate || *a == current))
    }

    pub fn on_key(&mut self, key: &str, down: bool, now: Instant) -> Decision {
        let mut d = Decision::default();

        if down {
            let repeat = !self.held.insert(key.to_string());
            if repeat {
                d.swallow = self.swallowed.contains(key);
                return d;
            }
        } else {
            let was_held = self.held.remove(key);
            if self.swallowed.remove(key) {
                d.swallow = true;
            }
            if !was_held {
                // Spurious/duplicate release (e.g. a key-up we never saw go down): ignore.
                return d;
            }
        }

        if self.recording_shortcut {
            if down {
                self.recorded_peak.insert(key.to_string());
                if key != "Escape" && !is_modifier(key) {
                    self.swallowed.insert(key.to_string());
                    d.swallow = true;
                }
            } else if self.held.is_empty() && !self.recorded_peak.is_empty() {
                let combo: Vec<String> = std::mem::take(&mut self.recorded_peak).into_iter().collect();
                self.recording_shortcut = false;
                if combo != ["Escape"] {
                    self.recorded = Some(combo);
                }
            }
            return d;
        }

        let swallow_this = |d: &mut Decision, sw: &mut BTreeSet<String>| {
            if !is_modifier(key) {
                sw.insert(key.to_string());
                d.swallow = true;
            }
        };

        match self.state.clone() {
            State::Idle => {
                if down {
                    if let Some(action) = self.binding_for(&self.held) {
                        d.outputs.push(Output::Start(action));
                        self.state = State::Held { action, since: now, started: now };
                        swallow_this(&mut d, &mut self.swallowed);
                    }
                }
            }
            State::Held { action, since, started } => {
                if down {
                    if key == "Escape" {
                        d.outputs.push(Output::Cancel);
                        self.state = State::Suppressed;
                        swallow_this(&mut d, &mut self.swallowed);
                    } else if let Some(next) = self.binding_for(&self.held) {
                        if next != action {
                            d.outputs.push(Output::Switch(next));
                        }
                        self.state = State::Held { action: next, since: now, started };
                        swallow_this(&mut d, &mut self.swallowed);
                    } else if !self.is_prefix_of_binding(&self.held) {
                        // Fn + some other key: not for us.
                        d.outputs.push(Output::Cancel);
                        self.state = State::Suppressed;
                    }
                } else {
                    // Any key of the combo released.
                    if self.toggle || now.duration_since(since) < self.tap_threshold {
                        d.outputs.push(Output::HandsFree);
                        self.state = State::HandsFree { action, started };
                    } else {
                        d.outputs.push(Output::Stop);
                        self.state = if self.held.is_empty() { State::Idle } else { State::Suppressed };
                    }
                }
            }
            State::HandsFree { action, started } => {
                if down {
                    // Debounce: a "finish" within 350 ms of starting is key bounce / a duplicate
                    // event, not a real second press.
                    let bounce = now.duration_since(started) < Duration::from_millis(350);
                    if key == "Escape" {
                        d.outputs.push(Output::Cancel);
                        self.state = State::Suppressed;
                        swallow_this(&mut d, &mut self.swallowed);
                    } else if self.is_finish_combo(&self.held, action) && !bounce {
                        d.outputs.push(Output::Stop);
                        self.state = State::Suppressed;
                        swallow_this(&mut d, &mut self.swallowed);
                    }
                }
            }
            State::Suppressed => {
                if self.held.is_empty() {
                    self.state = State::Idle;
                }
            }
        }
        d
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine() -> Engine {
        Engine::new(&Shortcuts::default())
    }

    #[test]
    fn toggle_mode_press_starts_press_again_stops() {
        let mut e = engine();
        let t0 = Instant::now();
        assert_eq!(e.on_key("Fn", true, t0).outputs, vec![Output::Start(Action::Dictate)]);
        // Even a long press keeps recording after release.
        assert_eq!(e.on_key("Fn", false, t0 + Duration::from_secs(2)).outputs, vec![Output::HandsFree]);
        assert!(e.is_active());
        assert_eq!(e.on_key("Fn", true, t0 + Duration::from_secs(9)).outputs, vec![Output::Stop]);
        assert!(e.on_key("Fn", false, t0 + Duration::from_secs(9)).outputs.is_empty());
        assert!(!e.is_active());
    }

    #[test]
    fn bounce_right_after_start_does_not_stop() {
        let mut e = engine();
        let t0 = Instant::now();
        e.on_key("Fn", true, t0);
        e.on_key("Fn", false, t0 + Duration::from_millis(60));
        assert!(e.on_key("Fn", true, t0 + Duration::from_millis(90)).outputs.is_empty());
        e.on_key("Fn", false, t0 + Duration::from_millis(110));
        assert!(e.is_active());
        assert_eq!(e.on_key("Fn", true, t0 + Duration::from_secs(3)).outputs, vec![Output::Stop]);
    }

    #[test]
    fn duplicate_release_is_ignored() {
        let mut e = engine();
        let t0 = Instant::now();
        e.on_key("Fn", true, t0);
        e.on_key("Fn", false, t0 + Duration::from_millis(100));
        // A second key-up for the same press must not change anything.
        assert!(e.on_key("Fn", false, t0 + Duration::from_millis(120)).outputs.is_empty());
        assert!(e.is_active());
    }

    #[test]
    fn hold_fn_is_push_to_talk_in_hybrid_mode() {
        let mut e = engine();
        e.set_toggle_mode(false);
        let t0 = Instant::now();
        assert_eq!(e.on_key("Fn", true, t0).outputs, vec![Output::Start(Action::Dictate)]);
        assert_eq!(e.on_key("Fn", false, t0 + Duration::from_secs(2)).outputs, vec![Output::Stop]);
        assert!(!e.is_active());
    }

    #[test]
    fn tap_fn_is_hands_free_then_fn_stops() {
        let mut e = engine();
        e.set_toggle_mode(false);
        let t0 = Instant::now();
        e.on_key("Fn", true, t0);
        assert_eq!(e.on_key("Fn", false, t0 + Duration::from_millis(120)).outputs, vec![Output::HandsFree]);
        assert!(e.is_active());
        assert_eq!(e.on_key("Fn", true, t0 + Duration::from_secs(5)).outputs, vec![Output::Stop]);
        assert!(e.on_key("Fn", false, t0 + Duration::from_secs(5)).outputs.is_empty());
        // Next press starts again.
        assert_eq!(e.on_key("Fn", true, t0 + Duration::from_secs(6)).outputs, vec![Output::Start(Action::Dictate)]);
    }

    #[test]
    fn fn_shift_switches_to_translate_and_space_is_swallowed_for_ask() {
        let mut e = engine();
        let t0 = Instant::now();
        e.on_key("Fn", true, t0);
        assert_eq!(e.on_key("ShiftLeft", true, t0).outputs, vec![Output::Switch(Action::Translate)]);
        let mut e = engine();
        e.on_key("Fn", true, t0);
        let d = e.on_key("Space", true, t0);
        assert_eq!(d.outputs, vec![Output::Switch(Action::Ask)]);
        assert!(d.swallow);
        let d = e.on_key("Space", false, t0 + Duration::from_millis(100));
        assert!(d.swallow);
        assert_eq!(d.outputs, vec![Output::HandsFree]);
        e.on_key("Fn", false, t0 + Duration::from_millis(150));
        // Finish with Fn.
        assert_eq!(e.on_key("Fn", true, t0 + Duration::from_secs(3)).outputs, vec![Output::Stop]);
    }

    #[test]
    fn fn_with_other_key_cancels_and_passes_through() {
        let mut e = engine();
        let t0 = Instant::now();
        e.on_key("Fn", true, t0);
        let d = e.on_key("ArrowLeft", true, t0);
        assert_eq!(d.outputs, vec![Output::Cancel]);
        assert!(!d.swallow);
        assert!(e.on_key("ArrowLeft", false, t0).outputs.is_empty());
        assert!(e.on_key("Fn", false, t0).outputs.is_empty());
        assert_eq!(e.on_key("Fn", true, t0).outputs, vec![Output::Start(Action::Dictate)]);
    }

    #[test]
    fn escape_cancels_hands_free() {
        let mut e = engine();
        let t0 = Instant::now();
        e.on_key("Fn", true, t0);
        e.on_key("Fn", false, t0 + Duration::from_millis(50));
        let d = e.on_key("Escape", true, t0 + Duration::from_secs(1));
        assert_eq!(d.outputs, vec![Output::Cancel]);
        assert!(d.swallow);
    }

    #[test]
    fn records_a_shortcut() {
        let mut e = engine();
        let t0 = Instant::now();
        e.begin_shortcut_recording();
        e.on_key("ControlRight", true, t0);
        e.on_key("KeyD", true, t0);
        e.on_key("KeyD", false, t0);
        e.on_key("ControlRight", false, t0);
        assert_eq!(e.take_recorded(), Some(vec!["ControlRight".to_string(), "KeyD".to_string()]));
    }
}
