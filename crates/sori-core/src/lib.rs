//! Platform-independent core of Sori.
//!
//! Everything here is plain Rust with no OS-specific code so it can be shared by the
//! desktop shell (Tauri, macOS/Windows) and, later, an Android IME through JNI/UniFFI.

pub mod audio;
pub mod devterms;
pub mod hotkey;
pub mod llm;
pub mod pipeline;
pub mod prompts;
pub mod settings;
pub mod store;
pub mod stt;
pub mod text;

pub use pipeline::{AskAction, Context, Mode, Outcome};
pub use settings::Settings;
