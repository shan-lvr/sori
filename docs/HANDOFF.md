# Sori — status & handoff

_Last updated: 2026-09-23. Written for whoever (human or coding agent) picks this up next — in particular for **verifying and finishing the Windows version on a real Windows PC**._

Sori is a voice-dictation app: press a shortcut in any text field, speak, and a cleaned-up version is pasted at the cursor. The owner uses it **mainly to dictate prompts to AI coding agents** (Claude Code, Codex, Cursor) instead of typing them; chat, email and notes are secondary. Everything runs on-device by default. See [README.md](../README.md) for the user-facing overview.

---

## 1. Where things stand

| Area | macOS (Apple Silicon) | Windows 10/11 |
|---|---|---|
| Builds | ✅ locally + CI (`.dmg`) | ✅ CI only (`nsis` installer) — never built on a real PC |
| Runs / tested by a human | ✅ daily use | ❌ **not run on real hardware yet** |
| Global shortcuts | ✅ CGEventTap, `Fn` (swallows the 🌐 action) | ⚠️ implemented (`WH_KEYBOARD_LL`/`WH_MOUSE_LL`), default `Ctrl+Win`, untested |
| Paste at cursor | ✅ ⌘V via CGEvent | ⚠️ implemented (`SendInput` Ctrl+V), untested |
| Focused-field / selection detection | ✅ Accessibility API | ⚠️ only the system caret (`GetGUIThreadInfo`) + Ctrl+C for selection |
| HUD voice bar / answer card | ✅ | ⚠️ untested (transparent, always-on-top, non-focusable, click-through) |
| On-device speech (Whisper) | ✅ Metal | ⚠️ Vulkan build compiles in CI, untested |
| On-device text model (llama-server) | ✅ Metal, measured | ⚠️ Vulkan sidecar compiles in CI, untested |
| Model downloads (resume/verify) | ✅ tested (pause, kill -9, corrupt file) | shared code, untested on Windows |
| UI (English/Korean) | ✅ | same code; Windows-specific wording handled (`isWin`) |
| Code signing | self-signed "Sori Dev" (local) / ad-hoc (CI); not notarized | unsigned → SmartScreen warning |

### Done and verified on macOS
- Dictate / Translate / Ask anything, toggle or hold-to-talk, Esc cancels, 9-minute cap.
- HUD voice bar (recording / transcribing / thinking), answer card for Ask.
- History (retry with the same audio, copy, export audio, delete; retention), Dictionary, Insights (time saved, streaks, heatmap).
- English default UI with full Korean translation (`L("English", "한국어")` helper in `desktop/src/i18n.ts`; Rust side uses `controller::tr`).
- One-click "Set up everything" on Home: permissions + both model downloads.
- On-device speech: Whisper large-v3-turbo q5_0 (574 MB). Korean CER ~2.8%, ~0.5–1 s/sentence on M4 Pro.
- On-device text model: llama.cpp `llama-server` sidecar + **Gemma 4 E2B** (3.1 GB, default) or Kanana-2 1.3B (0.85 GB). 0.4–1.8 s per dictation on M4 Pro GPU.
- Model downloader: per-model state, parallel downloads, HTTP Range resume (also after quitting), automatic retries with backoff, disk-space check, SHA-256 verification, pinned Hugging Face revisions, pause/resume/cancel/delete, progress in sidebar + Home + Settings.
- **Admin mode** (owner only, `desktop/src-tauri/src/admin.rs`): cloud engines — ElevenLabs Scribe v2 (speech), OpenRouter (text; default `google/gemini-3.8-flash`) — API keys and cloud model choice require unlocking with the owner's password. Locked ⇒ the backend forces Local AI and clears keys (`admin::enforce_policy`, applied on load, save and lock). Builds bake a salted PBKDF2-SHA256 verifier (600k rounds) from `SORI_ADMIN_PASSWORD` (`.env.local` or CI secret); personal builds also bake the owner's keys AES-256-GCM-encrypted under the password-derived key (`build.rs`, `src/admin_crypto.rs`) — no plaintext keys in any binary. Unlock state persists per device in `admin.key` until Lock. Check: `SORI_ADMIN_PASSWORD=… cargo run -p sori --example admin_check -- <password>`.
- **Ask anything is disabled on Local AI** (a small local model can't answer or look things up): shown greyed out with "API mode only — admin"; `Settings::ask_available()`; the controller refuses to start an Ask session (HUD notice) and ignores a mid-recording switch to Ask; `pipeline::process_text` also refuses.
- UI names local models simply: "Local AI — Whisper (574 MB)", "Gemma (3.1 GB)", "Kanana (0.85 GB)" — no version/quantization details (owner's request).
- Developer mode: ~200 dev terms as recognition hints, tech-term restoration in cleanup, per-app tone.
- Prompts tuned for the main use case (coding-agent prompts) — see §4.

### Removed / abandoned (don't resurrect without a reason)
- **Claude Code CLI as the text model** — `claude -p` was 2.5–3.5 s (Haiku 4.5 always "thinks"; `MAX_THINKING_TOKENS=0` got it to ~1.6–2.3 s, still ~1 s of CLI overhead from its post-turn summary). Replaced by the on-device model.
- Local STT alternatives tested and rejected: SenseVoice, Moonshine-ko, Fun-ASR-Nano, Qwen3-ASR, Cohere Transcribe, Whisper small (details in `docs/NOTES.ko.md`).
- Local text models rejected: Qwen3.5 0.8B/2B (summarize, copy example text into answers), LFM2.5 1.2B and EXAONE 4.0 1.2B (answer/summarize instead of cleaning up). Qwen3.5 4B is good but ~2× slower than Gemma 4 E2B.

---

## 2. Windows verification checklist

Work through this on a real Windows PC. Each item says what to expect and where the code is. Log file: `%LOCALAPPDATA%\com.seyoon.sori\logs\sori.log`; data (settings, models, history): `%APPDATA%\com.seyoon.sori\`; llama-server output: `%APPDATA%\com.seyoon.sori\llama-server.log`.

### Build
- [ ] Easiest: download the `sori-windows` artifact from the latest green run of the `build` workflow on GitHub Actions and install it.
- [ ] Local build: VS 2022 Build Tools (C++), CMake, LLVM (`LIBCLANG_PATH` for bindgen), Vulkan SDK, Rust stable, Node 22, Git Bash. Then:
  ```bash
  npm --prefix desktop ci
  ./scripts/build-llama-server.sh          # Git Bash; builds binaries/llama-server-x86_64-pc-windows-msvc.exe (Vulkan, static CRT)
  cd desktop && npx tauri build --bundles nsis
  ```
  Dev loop: `cd desktop && npx tauri dev` (the sidecar must exist first).

### Install & first run
- [ ] Installer runs per-user without an admin prompt (`bundle.windows.nsis.installMode = currentUser`). SmartScreen warning is expected (unsigned).
- [ ] Main window opens; Home shows "Get set up" with Microphone, speech model, text model (no Accessibility step on Windows).
- [ ] Settings → AI shows only Local AI; Admin mode unlocks with the owner's password only if the CI build had the `SORI_ADMIN_PASSWORD` secret (otherwise "not available in this build").
- [ ] "Set up everything" → Windows mic consent if needed; both downloads start in parallel; sidebar shows two pills with %.
- [ ] Quit mid-download (tray → Quit), relaunch → downloads resume from the same % (Range). Pull the network cable → "retrying (n/5)", then resumes.
- [ ] After downloads: log shows `local stt loaded …` and `llama-server ready: gemma-4-e2b on :<port>` then `local llm primed`. **Record the startup time and whether Vulkan picked the GPU** (see `llama-server.log`: look for `ggml_vulkan: Found N Vulkan devices` and `offloaded N/N layers to GPU`).

### Shortcuts (`desktop/src-tauri/src/platform/windows.rs`)
- [ ] `Ctrl+Win` (press, speak, press again) starts/stops dictation from any app. HUD appears bottom-center of the monitor under the mouse.
- [ ] Releasing Win after `Ctrl+Win` must **not** open the Start menu (we inject VK 0xE8 as a mask key while Win is held). A lone Win tap must still open Start.
- [ ] `Ctrl+Win+Shift` translate, `Ctrl+Win+Space` Ask, `Esc` cancels.
- [ ] Settings → Shortcuts → Add: recording a new combo works (mouse middle/X buttons too). Korean keyboards: the 한/영 key reports as `Lang1` — make sure it doesn't break anything.
- [ ] While typing normally, no keys are lost or delayed (the LL hook must return fast). Hooks can be silently removed by Windows if the callback is slow (`LowLevelHooksTimeout`) — if shortcuts stop working after a while, that's the suspect.

### Paste & context
Test in: Notepad, VS Code (editor + Copilot/agent chat), Cursor, Windows Terminal (with `claude`/`codex` running), Chrome text area, Slack/Discord (Electron), the Claude desktop app, Outlook.
- [ ] Dictated text is pasted at the cursor; focus returns to the target app (`LAST_HWND` + `SetForegroundWindow`).
- [ ] With "Also copy to clipboard" OFF, the previous clipboard text is restored ~0.7 s later.
- [ ] App category detection: `bundle_id` is `win:<exe name>`; categories in `crates/sori-core/src/prompts.rs::classify` include Windows exes (e.g. `win:code.exe`, `windowsterminal`, `win:claude.exe`). Check History shows the right app name, and that terminal + "claude"/"codex" window titles are treated as AI-agent prompts.
- [ ] Ask with selected text (Ctrl+C capture via `copy_selection`): "make this more polite" should replace the selection in editable fields. Known gap: without UI Automation we only know a field is editable when there is a system caret — Chromium/Electron apps often have none, so Ask falls back to the answer card. Consider implementing UI Automation (`IUIAutomation::GetFocusedElement`, `ValuePattern`/`TextPattern`).
- [ ] Elevated (admin) target apps can't receive `SendInput` from a non-elevated Sori — expected Windows behavior; make sure it fails gracefully.

### Overlays
- [ ] HUD is transparent (no white box), always on top, does **not** steal focus from the app being typed into, and is click-through except in hands-free mode (buttons clickable).
- [ ] Answer card (Ask) shows, can be closed with Esc/✕, returns focus to the previous app.

### On-device models on Windows GPUs
- [ ] Whisper (whisper-rs with `vulkan`) and llama-server (Vulkan) both use the GPU. Test on **integrated graphics** (Intel Iris Xe/Arc, AMD 780M) — target: text cleanup ≈ 2–3 s per sentence with Gemma 4 E2B; measure and record in `docs/NOTES.ko.md`.
- [ ] If `vulkan-1.dll` is missing (no GPU driver), both fail to start. There is **no CPU fallback build yet** — decide whether to ship a CPU-only `llama-server` as a fallback (see §3).
- [ ] Memory: ~4 GB with both models loaded. Check behavior on an 8 GB machine.

### Misc
- [ ] Tray menu (Open / Settings / Quit) in the UI language; launch-at-login; "Show in taskbar" toggle (`set_dock_visible` → `skip_taskbar`).
- [ ] Start/stop sounds (`C:\Windows\Media\Speech On.wav` / `Speech Off.wav`).
- [ ] Microphone privacy denied → Home shows "denied" with "Open Settings" (`ms-settings:privacy-microphone`). Detection reads `HKCU/HKLM\…\CapabilityAccessManager\ConsentStore\microphone[\NonPackaged]`.
- [ ] Single instance: launching again focuses the existing window.

---

## 3. Known gaps / not done yet

**Windows**
- Never run on real hardware (everything in §2).
- Text-field / selection detection is caret-based only; UI Automation not implemented.
- No CPU fallback when Vulkan is unavailable (llama-server and whisper). Options: ship a second, CPU-only `llama-server-cpu.exe` and retry with it when the Vulkan one exits at startup; for Whisper, build whisper-rs with a runtime backend choice.
- "Mute while dictating" is a no-op on Windows (`mute_output`); needs the Core Audio `IAudioEndpointVolume` API.
- Installer isn't code-signed (SmartScreen). Not published to a Release yet (push a `v*` tag to make CI attach the `.exe` and `.dmg`).
- Default shortcut choice (`Ctrl+Win`) should be validated with the owner; Typeless' Windows default is reported as either Right Alt or Ctrl+Win.

**On-device text model quality** (Gemma 4 E2B, 2B-class)
- Reliable: keeps language and 반말/존댓말, keeps every request/constraint/hedge, removes fillers, fixes most tech terms, formats lists.
- Weaker than cloud Gemini at deep restating: long, meta-level requests are only lightly reorganized; some misheard words survive (e.g. "폴리시" → "Politeness", "스트릭 모드" not always → Strict Mode). Owners who want the strongest polish can switch Settings → AI → Text AI to OpenRouter.
- Ask anything is intentionally disabled with the local model (see §1); it works in API mode (admin).

**Everything else**
- macOS: not notarized (Gatekeeper "Open Anyway" on first launch); Intel Macs not built.
- No auto-update.
- Streaming transcription (text appearing while speaking) not implemented.
- Dictionary auto-learning from corrections not implemented.
- Android (IME reusing `sori-core`) not started.
- First llama-server start after install/update compiles GPU shaders (~20 s on Metal) — happens in the background at app start; later starts ~1 s.

---

## 4. How cleanup prompting works (important for quality work)

Code: `crates/sori-core/src/prompts.rs`, `pipeline.rs`.

- **Purpose is stated explicitly.** Both prompt families say the speaker is a developer who mostly dictates prompts for AI coding agents; unclear words are read in that light. Per-app styles (`AppCategory::style`) turn Code/AI destinations into "a clear instruction the agent can act on: situation → request → constraints; numbered list for multiple requests".
- **Cloud models** (OpenRouter) get the long `POLISH_SYSTEM` (default "Polished" style) or `DICTATE_SYSTEM` ("Faithful"), plus the developer block, per-app style, dictionary and the user's own style notes.
- **Small local models** get `LOCAL_DICTATE_SYSTEM` (short) + 5 worked examples as prior chat turns (`local_examples`). Each input carries a tag computed in code, e.g. `[Write in Korean, casual 반말 endings like ~해/~줘/~야 — no ~요 · a prompt for a coding agent]`:
  - language from the Hangul/Latin ratio (`language_hint`) — without it the model translated English input into Korean;
  - speech level from sentence endings (`korean_register`: -요, -ㅂ니다/-ㅂ니까; careful: 그러니까 / 아니다 are not polite) — without it the model drifted into 존댓말;
  - destination from the app category (`local_destination`).
  The long cloud prompt made every small model summarize, answer or translate; this format fixed it.
- The llama-server keeps the system prompt + examples in its KV cache (`cache_prompt`, one slot), so only the new words are processed per dictation.
- Safety nets in `pipeline.rs`: if the model "answers" instead of rewriting (`text::looks_like_assistant_reply`) or fails, the raw transcript is inserted and History shows why.

**Evaluate before/after any prompt change** (same code path as the app):
```bash
# cloud (needs OPENROUTER_API_KEY) and/or a local llama-server on :8080
cargo run -p sori-core --example polish_eval -- google/gemini-3.8-flash local:gemma@http://127.0.0.1:8080
CASE=3 cargo run -p sori-core --example polish_eval -- …   # one case
```
The cases in `crates/sori-core/examples/polish_eval.rs` are mostly real coding-agent dictation plus a chat and an email case. Check: language, 반말/존댓말, every constraint ("don't commit", "바로 고치지 말고") and hedge ("maybe", "아마") survives, nothing invented, tech terms fixed.

---

## 5. Code map

```
crates/sori-core/            platform-independent (reusable for Android)
  hotkey.rs                  shortcut state machine (toggle / hold / combos / cancel)
  prompts.rs                 all prompts, app classification, language/register tags
  pipeline.rs                STT → LLM → guards; one-step (audio → Gemini) experiment
  llm.rs                     OpenRouter client, LocalServer (llama-server) client, LlmCall trait
  stt.rs, devterms.rs        ElevenLabs client, developer vocabulary
  store.rs                   SQLite history, dictionary, stats
  settings.rs                settings schema + defaults (local STT + local LLM by default)
desktop/src-tauri/src/
  controller.rs              session orchestration, HUD/card, model sync, delivery
  commands.rs                Tauri commands for the UI
  models.rs                  model catalog (ids, pinned URLs, sizes, SHA-256) + downloader
  local_stt.rs               whisper.cpp wrapper
  local_llm.rs               llama-server sidecar manager + LlmCall impl
  recorder.rs                cpal mic capture
  platform/macos/*           CGEventTap, AX, paste, overlays
  platform/windows.rs        LL hooks, SendInput, foreground app, mic privacy, overlays
desktop/src/                 React UI (main/*, overlay/Hud.tsx, overlay/Card.tsx, i18n.ts, mock.ts)
scripts/                     build-mac.sh (personal), build-team.sh (shareable DMG), build-llama-server.sh
.github/workflows/build.yml  test → macOS DMG + Windows NSIS (+ Release on v* tags)
```

Useful harnesses (same code paths as the app):
```bash
cargo test -p sori-core
npx --prefix desktop tsc --noEmit -p desktop
cargo run --release -p sori --example local_llm -- <models_dir> gemma-4-e2b          # sidecar + cleanup/translate/ask
cargo run --release -p sori --example local_stt_download -- <dir> [pause_s] [model_id]
cargo run --release -p sori --example local_stt_eval -- <models_dir> <data_dir> [prompts]
npm --prefix desktop run dev   # UI with mock backend: http://localhost:1420/?window=main|hud|card
```

## 6. Rules of the road
- Never commit `.env.local` or any key. Share `build-team.sh` / CI builds only — a `build-mac.sh` build carries the owner's keys (encrypted, but still). `build-team.sh` refuses to package if a plaintext key leaks.
- Cloud features must stay behind admin mode: anything new that uses an API key must respect `admin::enforce_policy` / `app.admin.is_unlocked()`.
- Model downloads must stay pinned (URL revision + size + SHA-256 in `models.rs`); update all three together.
- The llama.cpp version is pinned in `scripts/build-llama-server.sh` (`LLAMA_CPP_TAG`). Upgrade deliberately and re-run the evals.
- Keep UI strings bilingual via `L("English", "한국어")`.
