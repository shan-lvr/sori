# Sori

**Speak, don't type.** Press a shortcut in any text field, talk the way you think, and Sori pastes a clean, concise version at your cursor — fillers, false starts and rambling removed, your intent kept.

- **Speech recognition on-device** — Whisper large-v3-turbo runs locally (Metal on macOS, Vulkan on Windows). Your voice never leaves your computer. ElevenLabs Scribe is an optional cloud engine.
- **Cleanup with your own Claude subscription** — through the Claude Code CLI, kept warm in the background so each dictation only waits for the model. OpenRouter (API key) is the alternative.
- **Translate** as you speak, and **Ask anything** about selected text ("make this more polite", "summarize").
- Developer-aware: restores misheard tech terms (`useEffect`, React Query, `git rebase`…), per-app tone (email vs chat vs code editor vs AI prompt).
- English and Korean UI.

| | macOS (Apple Silicon) | Windows 10/11 |
|---|---|---|
| Dictate | `Fn` | `Ctrl` + `Win` |
| Translate | `Fn` + `Left Shift` | `Ctrl` + `Win` + `Shift` |
| Ask anything | `Fn` + `Space` | `Ctrl` + `Win` + `Space` |
| Cancel | `Esc` | `Esc` |

Press once to start, press again to finish (or switch to hold-to-talk in Settings → Shortcuts). Shortcuts, including mouse buttons, are configurable.

## Install

Download the latest build from [Releases](../../releases).

**macOS** — open the `.dmg` and drag Sori to Applications. The build isn't notarized, so the first launch is blocked: open **System Settings → Privacy & Security** and click **Open Anyway** (or run `xattr -dr com.apple.quarantine /Applications/Sori.app`).

**Windows** — run `Sori_x.y.z_x64-setup.exe` (installs for the current user, no admin needed). If SmartScreen warns, click **More info → Run anyway**.

Then, on the Home screen, click **Set up everything**. It:

1. asks for Microphone (and on macOS, Accessibility) permission,
2. downloads the on-device speech model (574 MB, resumable, SHA-256 verified),
3. installs the Claude Code CLI if it's missing (official installer, no admin rights),
4. opens your browser to sign in to Claude — Sori finishes automatically when you approve (or paste the code the page shows).

You need a Claude plan that includes Claude Code. Usage counts toward that plan's limits.

## How it works

```
shortcut ─▶ mic (cpal) ─▶ speech-to-text ─────────────▶ cleanup / translate / ask ─▶ paste at cursor
                          on-device Whisper (default)    Claude Code CLI (default)     + clipboard (optional)
                          or ElevenLabs Scribe           or OpenRouter
```

- `crates/sori-core` — platform-independent Rust: hotkey state machine, prompts, pipeline, LLM/STT clients, SQLite history & stats.
- `desktop/src-tauri` — Tauri 2 shell: recorder, on-device Whisper + model downloader, Claude Code CLI integration, platform layers (`platform/macos`: CGEventTap, Accessibility, ⌘V · `platform/windows.rs`: low-level hooks, SendInput).
- `desktop/src` — React UI (Home, History, Dictionary, Settings, the voice-bar HUD and answer card).

The model download survives quitting the app (it resumes with an HTTP Range request), retries network hiccups automatically, checks free disk space first and verifies the file's SHA-256 before use. Progress is shown in the sidebar, on Home and in Settings → AI, with pause / resume / retry / discard.

## Build

Requirements: Rust (stable), Node 20+, and

- macOS: Xcode Command Line Tools.
- Windows: Visual Studio Build Tools (C++), CMake, LLVM (for bindgen) and the [Vulkan SDK](https://vulkan.lunarg.com/).

```bash
npm --prefix desktop ci

# macOS — personal build, installs to /Applications (signs with a local self-signed
# "Sori Dev" identity so macOS keeps permissions across rebuilds)
./scripts/build-mac.sh

# macOS — shareable build without any API keys → dist/Sori-<version>-arm64.dmg
./scripts/build-team.sh

# Windows
cd desktop && npx tauri build --bundles nsis
```

`build-mac.sh` bakes default API keys from a git-ignored `.env.local` (`ELEVENLABS_API_KEY=…`, `OPENROUTER_API_KEY=…`) for your own machine only. **Never distribute that build** — use `build-team.sh` / CI builds, which contain no keys and default to on-device speech + Claude Code.

UI-only development with mock data: `npm --prefix desktop run dev`, then open `http://localhost:1420/?window=main` (or `hud`, `card`).

Tests: `cargo test -p sori-core` · `npx --prefix desktop tsc --noEmit -p desktop`

Useful harnesses (same code paths as the app):

```bash
cargo run -p sori --example claude_cli -- status|install|bench     # Claude Code CLI integration
cargo run --release -p sori --example local_stt_download -- <dir>  # resumable model download
cargo run -p sori-core --example polish_eval -- google/gemini-3.8-flash   # cleanup quality (needs OPENROUTER_API_KEY)
```

## Status

- macOS: daily-driven.
- Windows: implemented and built in CI; not yet tested on real hardware. Text-field detection relies on the system caret, so some apps (browsers, Electron) are treated as "unknown" and always get a paste.
- Android: planned as a keyboard (IME) reusing `sori-core`.

Benchmarks and design notes (Korean): [docs/NOTES.ko.md](docs/NOTES.ko.md).
