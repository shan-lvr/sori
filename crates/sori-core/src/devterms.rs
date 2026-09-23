//! Developer vocabulary: sent to the recognizer as keyterms (so it hears "React Query" instead
//! of "리액트 쿼리" or "리액 퀘리") when developer mode is on.

use crate::settings::Settings;

pub const DEV_KEYTERMS: &[&str] = &[
    // languages & runtimes
    "Python", "JavaScript", "TypeScript", "Rust", "Go", "Kotlin", "Swift", "Java", "C++", "C#", "SQL", "HTML", "CSS",
    "Bash", "zsh", "Node.js", "Deno", "Bun", "WebAssembly",
    // frontend
    "React", "React Native", "React Query", "TanStack", "Next.js", "Vue", "Nuxt", "Svelte", "Tailwind", "Vite",
    "Webpack", "Redux", "Zustand", "shadcn", "Storybook", "useEffect", "useState", "useMemo", "useCallback", "props",
    "JSX", "SSR", "hydration", "Server Actions", "App Router", "Zod", "fetch", "fetching", "data fetching",
    "Error Boundary", "Suspense", "re-render", "state management",
    // backend & data
    "Express", "FastAPI", "Django", "Flask", "Spring", "NestJS", "GraphQL", "REST API", "gRPC", "WebSocket", "Redis",
    "PostgreSQL", "Postgres", "MySQL", "MongoDB", "SQLite", "Supabase", "Firebase", "Prisma", "Drizzle", "Kafka",
    "RabbitMQ", "Nginx", "ORM", "schema", "migration", "query",
    // infra & cloud
    "Docker", "Docker Compose", "Kubernetes", "k8s", "Helm", "Terraform", "AWS", "GCP", "Azure", "Lambda", "S3",
    "EC2", "CloudFront", "Cloudflare", "Vercel", "Netlify", "Datadog", "Sentry", "Grafana", "Prometheus",
    // workflow & tools
    "Git", "GitHub", "GitLab", "GitHub Actions", "CI/CD", "CI", "PR", "pull request", "merge", "rebase", "cherry-pick",
    "commit", "branch", "force push", "stash", "diff", "npm", "npm install", "pnpm", "yarn", "pip", "uv", "Homebrew",
    "Xcode", "VS Code", "Cursor", "Claude Code", "Codex", "Copilot", "Jira", "Linear", "Figma", "Postman", "cURL",
    "ESLint", "Prettier", "Jest", "Vitest", "Playwright", "Cypress", "unit test", "E2E", "lint", "refactor",
    "hotfix", "staging", "production", "deploy", "rollback", "localhost", "env", ".env", "dotenv",
    // AI / LLM
    "LLM", "GPT", "Claude", "Gemini", "OpenAI", "Anthropic", "OpenRouter", "ElevenLabs", "Hugging Face", "Whisper",
    "STT", "TTS", "RAG", "embedding", "fine-tuning", "prompt", "system prompt", "token", "context window", "MCP",
    "agent", "subagent", "tool call", "WER",
    // concepts & formats
    "API", "SDK", "CLI", "GUI", "JSON", "YAML", "TOML", "CSV", "JWT", "OAuth", "SSO", "UUID", "URL", "HTTP",
    "HTTPS", "CORS", "DNS", "SSH", "TLS", "endpoint", "payload", "callback", "Promise", "async", "await", "null",
    "undefined", "TypeError", "stack trace", "race condition", "deadlock", "mutex", "thread", "latency",
    "throughput", "cache", "webhook", "cron", "regex", "UTF-8", "Unicode",
    // platforms, desktop & XR
    "Tauri", "Electron", "WebView", "SwiftUI", "iOS", "Android", "macOS", "Windows", "Linux", "Unity", "Unreal",
    "OpenXR", "Meta Quest", "Vision Pro", "WebRTC", "FFmpeg", "Metal", "CoreML", "CGEventTap", "Accessibility API",
];

/// Keyterms for the recognizer: personal dictionary first (highest priority), then the
/// user's tech stack, then the built-in developer vocabulary.
pub fn stt_keyterms(s: &Settings, dictionary: &[String]) -> Vec<String> {
    let mut out: Vec<String> = dictionary.to_vec();
    if s.dev_mode {
        out.extend(tech_stack(s));
        out.extend(DEV_KEYTERMS.iter().map(|t| t.to_string()));
    }
    out
}

/// The user's stack from settings ("Unity, C#, Rust" → ["Unity", "C#", "Rust"]).
pub fn tech_stack(s: &Settings) -> Vec<String> {
    s.tech_stack
        .split([',', '\n', '·', '/'])
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .collect()
}
