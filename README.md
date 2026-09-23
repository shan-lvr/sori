# Sori

**Speak, don't type.** Press a shortcut in any text field, talk the way you think, and Sori pastes a clean, concise version at your cursor — fillers, false starts and rambling removed, your intent kept.

- **Fully on-device by default** — Whisper large-v3-turbo for speech, and a small language model (Gemma 4 E2B via llama.cpp) for cleanup. Metal on macOS, Vulkan on Windows (integrated graphics work). No keys, no account, nothing leaves your computer.
- **Optional cloud engines** — ElevenLabs Scribe for speech, OpenRouter (e.g. Gemini 3.8 Flash) for sharper writing.
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

Then, on the Home screen, click **Set up everything**. It asks for Microphone (and on macOS, Accessibility) permission and downloads the two on-device models in the background — Whisper (574 MB) and Gemma 4 E2B (3.1 GB). Downloads resume after a quit or a dropped connection and are SHA-256 verified. About 4 GB of RAM is used while both are loaded; 16 GB machines are comfortable.

## How it works

```
shortcut ─▶ mic (cpal) ─▶ speech-to-text ─────────────▶ cleanup / translate / ask ─▶ paste at cursor
                          on-device Whisper (default)    on-device LLM (default)       + clipboard (optional)
                          or ElevenLabs Scribe           or OpenRouter
```

The on-device LLM is llama.cpp's `llama-server`, bundled as a sidecar (a separate process, because whisper.cpp and llama.cpp each vendor ggml). Sori starts it on a random localhost port behind a random API key, keeps the model loaded, and caches the shared prompt prefix so each dictation only processes the new words. Small models get a short prompt with worked examples and an explicit language tag — with the large-model prompt they summarize, translate or answer instead of cleaning up.

| Local text model | Size | Speed (M4 Pro GPU) | Notes |
|---|---|---|---|
| **Gemma 4 E2B** (default) | 3.1 GB | 0.4–1.1 s | Keeps language, 반말/존댓말 and every detail; fixes tech terms. Apache-2.0 |
| Kanana-2 1.3B | 0.85 GB | 0.2–0.5 s | ~2× faster, lighter polishing, occasionally drops a detail. Kanana Open License |

Rejected in testing: Qwen3.5 0.8B/2B (summarize, copy example text), LFM2.5 1.2B and EXAONE 4.0 1.2B (answer or summarize instead of cleaning up); Qwen3.5 4B is good but ~2× slower than Gemma 4 E2B. On integrated graphics expect roughly 2–3 s per sentence with Gemma 4 E2B.

- `crates/sori-core` — platform-independent Rust: hotkey state machine, prompts, pipeline, LLM/STT clients, SQLite history & stats.
- `desktop/src-tauri` — Tauri 2 shell: recorder, on-device Whisper, the `llama-server` sidecar, model downloader, platform layers (`platform/macos`: CGEventTap, Accessibility, ⌘V · `platform/windows.rs`: low-level hooks, SendInput).
- `desktop/src` — React UI (Home, History, Dictionary, Settings, the voice-bar HUD and answer card).

The model download survives quitting the app (it resumes with an HTTP Range request), retries network hiccups automatically, checks free disk space first and verifies the file's SHA-256 before use. Progress is shown in the sidebar, on Home and in Settings → AI, with pause / resume / retry / discard.

## Build

Requirements: Rust (stable), Node 20+, and

- macOS: Xcode Command Line Tools.
- Windows: Visual Studio Build Tools (C++), CMake, LLVM (for bindgen) and the [Vulkan SDK](https://vulkan.lunarg.com/).

```bash
npm --prefix desktop ci
./scripts/build-llama-server.sh   # static llama-server sidecar (pinned llama.cpp tag); build scripts run it too

# macOS — personal build, installs to /Applications (signs with a local self-signed
# "Sori Dev" identity so macOS keeps permissions across rebuilds)
./scripts/build-mac.sh

# macOS — shareable build without any API keys → dist/Sori-<version>-arm64.dmg
./scripts/build-team.sh

# Windows
cd desktop && npx tauri build --bundles nsis
```

`build-mac.sh` bakes default API keys from a git-ignored `.env.local` (`ELEVENLABS_API_KEY=…`, `OPENROUTER_API_KEY=…`) for your own machine only. **Never distribute that build** — use `build-team.sh` / CI builds, which contain no keys.

UI-only development with mock data: `npm --prefix desktop run dev`, then open `http://localhost:1420/?window=main` (or `hud`, `card`).

Tests: `cargo test -p sori-core` · `npx --prefix desktop tsc --noEmit -p desktop`

Useful harnesses (same code paths as the app):

```bash
cargo run --release -p sori --example local_llm -- <models_dir> gemma-4-e2b   # on-device cleanup via the sidecar
cargo run --release -p sori --example local_stt_download -- <dir> [pause_s] [model_id]   # resumable download
cargo run -p sori-core --example polish_eval -- google/gemini-3.8-flash local:gemma@http://127.0.0.1:8080   # compare models
```

## Status

- macOS: daily-driven.
- Windows: implemented and built in CI; not yet tested on real hardware. Text-field detection relies on the system caret, so some apps (browsers, Electron) are treated as "unknown" and always get a paste.
- Android: planned as a keyboard (IME) reusing `sori-core`.

Benchmarks and design notes (Korean): [docs/NOTES.ko.md](docs/NOTES.ko.md).
