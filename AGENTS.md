# Notes for coding agents

Start with [docs/HANDOFF.md](docs/HANDOFF.md): current status, what's verified, the Windows verification checklist, known gaps, and how the cleanup prompts work. User-facing overview: [README.md](README.md).

- Stack: Rust workspace (`crates/sori-core` + Tauri 2 app in `desktop/src-tauri`) and a React/TypeScript UI in `desktop/src`.
- Checks before committing: `cargo test -p sori-core`, `cargo check -p sori`, `npx --prefix desktop tsc --noEmit -p desktop`.
- Prompt changes: run `cargo run -p sori-core --example polish_eval -- …` before and after (see HANDOFF §4).
- The main use case is dictating prompts to AI coding agents — optimize cleanup for that.
- Never commit `.env.local` or keys; never distribute `scripts/build-mac.sh` builds.
- UI strings are bilingual: `L("English", "한국어")`.
