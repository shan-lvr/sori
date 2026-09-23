// Mock backend for browser-only UI development (`npm run dev`, open http://localhost:1420/?window=main|hud|card).
import type { DictWord, HistoryEntry, Settings, Stats } from "./api";

const now = Date.now();
const settings: Settings = {
  elevenlabs_api_key: "sk_mock",
  openrouter_api_key: "sk-or-mock",
  stt_model: "scribe_v2",
  stt_engine: "local",
  local_model: "whisper-large-v3-turbo-q5_0",
  stt_language: "",
  llm_model: "google/gemini-3.8-flash",
  ask_model: "google/gemini-3.8-flash",
  llm_provider: "local",
  local_llm_model: "gemma-4-e2b",
  cleanup_style: "polished",
  pipeline_mode: "two_step",
  one_step_model: "google/gemini-3.8-flash",
  shortcuts: { dictate: [["Fn"]], translate: [["Fn", "ShiftLeft"]], ask: [["Fn", "Space"]] },
  recording_mode: "toggle",
  translation_targets: [
    { code: "en-US", name: "English (US)" },
    { code: "ja-JP", name: "Japanese" },
  ],
  active_translation_target: 0,
  microphone: null,
  copy_to_clipboard: true,
  interaction_sounds: true,
  mute_when_dictating: false,
  launch_at_login: false,
  show_in_dock: true,
  appearance: (new URLSearchParams(location.search).get("theme") as Settings["appearance"]) ?? "system",
  interface_language: "en",
  history_retention: "forever",
  per_app_tone: true,
  custom_instructions: "",
  dev_mode: true,
  tech_stack: "",
  typing_wpm: 40,
  compose_factor: 2,
  manage_fn_key: false,
  onboarding_done: false,
};

const days = Array.from({ length: 16 }, (_, i) => {
  const d = new Date(now - (15 - i) * 86400000 * (i < 4 ? 3 : 1));
  const day = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
  return { day, words: 200 + ((i * 397) % 1400), audio_ms: 90000 + i * 20000, sessions: 3 + (i % 5) };
});
const stats: Stats = {
  total_words: 15712,
  total_audio_ms: 2 * 3600000 + 6 * 60000,
  avg_wpm: 187,
  time_saved_ms: 7 * 3600000 + 43 * 60000,
  active_days: 14,
  current_streak: 12,
  longest_streak: 12,
  days,
};

const history: HistoryEntry[] = [
  {
    id: "1",
    created_at: now - 5 * 60000,
    mode: "dictate",
    app_name: "Slack",
    bundle_id: "com.tinyspeck.slackmacgap",
    raw_text: "음 그러니까 내일 회의는 3시, 아니 4시로 옮기고, 어 할 일은 첫째 API 키 설정, 둘째 단축키, 셋째 기록 화면이야",
    output: "내일 회의는 4시로 옮기고, 할 일은 다음과 같아.\n\n1. API 키 설정\n2. 단축키\n3. 기록 화면",
    audio_path: "/tmp/a.wav",
    duration_ms: 14700,
    words: 16,
    language: "kor",
    status: "ok",
    error: null,
    stt_ms: 1012,
    llm_ms: 690,
    context_json: "{}",
    action: "insert",
  },
  {
    id: "2",
    created_at: now - 40 * 60000,
    mode: "translate",
    app_name: "Mail",
    bundle_id: "com.apple.mail",
    raw_text: "다음 주 화요일에 데모 가능하신지 여쭤보고 싶어요",
    output: "I wanted to check whether you'd be available for a demo next Tuesday.",
    audio_path: "/tmp/b.wav",
    duration_ms: 5200,
    words: 13,
    language: "kor",
    status: "ok",
    error: null,
    stt_ms: 640,
    llm_ms: 810,
    context_json: "{}",
    action: "insert",
  },
  {
    id: "3",
    created_at: now - 26 * 3600000,
    mode: "ask",
    app_name: "Safari",
    bundle_id: "com.apple.Safari",
    raw_text: "이 문단 세 줄로 요약해줘",
    output: "- 핵심 1\n- 핵심 2\n- 핵심 3",
    audio_path: "/tmp/c.wav",
    duration_ms: 2400,
    words: 4,
    language: "kor",
    status: "ok",
    error: null,
    stt_ms: 500,
    llm_ms: 1800,
    context_json: "{}",
    action: "answer",
  },
  {
    id: "4",
    created_at: now - 27 * 3600000,
    mode: "dictate",
    app_name: "Notion",
    bundle_id: "notion.id",
    raw_text: "",
    output: "",
    audio_path: "/tmp/d.wav",
    duration_ms: 8000,
    words: 0,
    language: "",
    status: "error",
    error: "ElevenLabs 요청 실패: error sending request",
    stt_ms: 0,
    llm_ms: 0,
    context_json: "{}",
    action: "insert",
  },
];

const dictionary: DictWord[] = ["ElevenLabs", "OpenRouter", "Typeless", "GetLuckyVR", "세윤", "Scribe", "Tauri", "WER"].map((term, i) => ({
  id: i + 1,
  term,
  source: i % 3 === 2 ? "auto" : "manual",
  created_at: now - i * 1000,
}));

// Simulated model downloads (~10 s each, one dropped connection at 40% to show automatic retry).
type MockDl = { id: string; state: string; downloaded: number; total: number; bytes_per_sec: number; error: string | null; attempt: number };
type MockModel = {
  id: string;
  kind: "stt" | "llm";
  label: string;
  size: number;
  license: string;
  installed: boolean;
  loaded: boolean;
  downloading: boolean;
  partial: number;
  paused: boolean;
  download: MockDl | null;
  stop: "" | "pause" | "cancel";
  failed: boolean;
};
const mk = (id: string, kind: "stt" | "llm", label: string, size: number, license: string): MockModel => ({
  id, kind, label, size, license, installed: false, loaded: false, downloading: false, partial: 0, paused: false, download: null, stop: "", failed: false,
});
const models: MockModel[] = [
  mk("whisper-large-v3-turbo-q5_0", "stt", "Whisper large-v3-turbo (q5)", 574041195, "MIT"),
  mk("gemma-4-e2b", "llm", "Gemma 4 E2B (Q4_K_M)", 3106738272, "Apache-2.0"),
  mk("kanana-2-1.3b", "llm", "Kanana-2 1.3B (Q4_K_M)", 853655776, "Kanana Open License"),
];
const findModel = (id: unknown) => models.find((m) => m.id === id) ?? models[0];
const emitDl = (m: MockModel, d: MockDl) => {
  m.download = d;
  window.dispatchEvent(new CustomEvent("mock:local-model-download", { detail: { ...d } }));
};
async function mockDownload(model: MockModel) {
  if (model.downloading) return;
  Object.assign(model, { downloading: true, paused: false, stop: "" });
  const base = { id: model.id, total: model.size, bytes_per_sec: 0, error: null, attempt: 0 };
  const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
  emitDl(model, { ...base, state: "connecting", downloaded: model.partial });
  await sleep(600);
  while (model.partial < model.size) {
    if (model.stop) break;
    if (!model.failed && model.partial > model.size * 0.4) {
      model.failed = true;
      emitDl(model, { ...base, state: "retrying", downloaded: model.partial, attempt: 1, error: "Connection interrupted" });
      await sleep(1500);
    }
    model.partial = Math.min(model.size, model.partial + model.size / 40);
    emitDl(model, { ...base, state: "downloading", downloaded: model.partial, bytes_per_sec: 56_000_000 });
    await sleep(250);
  }
  model.downloading = false;
  if (model.stop === "pause") {
    model.paused = true;
    emitDl(model, { ...base, state: "paused", downloaded: model.partial });
  } else if (model.stop === "cancel") {
    Object.assign(model, { partial: 0, download: null });
    window.dispatchEvent(new CustomEvent("mock:local-model-download", { detail: { ...base, state: "cancelled", downloaded: 0 } }));
  } else {
    emitDl(model, { ...base, state: "verifying", downloaded: model.size });
    await sleep(1200);
    Object.assign(model, { installed: true, loaded: true, partial: 0 });
    emitDl(model, { ...base, state: "done", downloaded: model.size });
  }
  model.stop = "";
}

export async function mockInvoke(cmd: string, args?: Record<string, unknown>): Promise<unknown> {
  switch (cmd) {
    case "retry_history": {
      await new Promise((r) => setTimeout(r, 900));
      const h = history.find((x) => x.id === args?.id)!;
      Object.assign(h, { status: "ok", error: null, output: h.output || "다시 처리된 결과 텍스트예요.", stt_ms: 480, llm_ms: 620 });
      return h;
    }
    case "delete_history": {
      const i = history.findIndex((x) => x.id === args?.id);
      if (i >= 0) history.splice(i, 1);
      return null;
    }
    case "delete_all_history":
      history.splice(0, history.length);
      return null;
    case "copy_text":
      await navigator.clipboard?.writeText(String(args?.text ?? "")).catch(() => {});
      return null;
    case "get_settings":
      return settings;
    case "save_settings":
      Object.assign(settings, args?.settings);
      return settings;
    case "get_permissions":
      return {
        accessibility: false,
        microphone: "unknown",
        fn_usage_type: 1,
        hotkeys_ready: false,
        fn_managed: false,
        typeless_running: true,
      };
    case "get_stats":
      return stats;
    case "list_history": {
      const f = args?.filter as string;
      return [...history].filter((h) => f === "all" || (f === "ask" ? h.mode === "ask" : h.mode !== "ask"));
    }
    case "list_dictionary":
      return dictionary;
    case "list_microphones":
      return ["MacBook Pro Microphone", "AirPods Pro"];
    case "local_models":
      return models.map(({ stop: _s, failed: _f, ...m }) => ({ ...m, download: m.download ? { ...m.download } : null }));
    case "download_local_model":
      mockDownload(findModel(args?.id));
      return null;
    case "pause_local_download":
      findModel(args?.id).stop = "pause";
      return null;
    case "cancel_local_download": {
      const m = findModel(args?.id);
      if (m.downloading) m.stop = "cancel";
      else Object.assign(m, { partial: 0, paused: false, download: null });
      return null;
    }
    case "delete_local_model":
      Object.assign(findModel(args?.id), { installed: false, loaded: false, paused: true, download: null });
      return null;
    case "app_info":
      return { version: "0.1.0", data_dir: "~/Library/Application Support/com.seyoon.sori", config_path: "…/settings.json", has_default_keys: true, platform: "macos", default_shortcuts: { dictate: [["Fn"]], translate: [["Fn", "ShiftLeft"]], ask: [["Fn", "Space"]] } };
    default:
      return null;
  }
}
