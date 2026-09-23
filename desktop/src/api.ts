import { call as invoke, isWin } from "./bridge";

export type Shortcut = string[];
export interface TranslationTarget {
  code: string;
  name: string;
}
export interface Settings {
  elevenlabs_api_key: string;
  openrouter_api_key: string;
  stt_model: string;
  stt_engine: "elevenlabs" | "local";
  local_model: string;
  stt_language: string;
  llm_model: string;
  ask_model: string;
  llm_provider: "local" | "openrouter";
  local_llm_model: string;
  cleanup_style: "polished" | "faithful";
  pipeline_mode: "two_step" | "one_step";
  one_step_model: string;
  shortcuts: { dictate: Shortcut[]; translate: Shortcut[]; ask: Shortcut[] };
  recording_mode: "toggle" | "hybrid";
  translation_targets: TranslationTarget[];
  active_translation_target: number;
  microphone: string | null;
  copy_to_clipboard: boolean;
  interaction_sounds: boolean;
  mute_when_dictating: boolean;
  launch_at_login: boolean;
  show_in_dock: boolean;
  appearance: "system" | "light" | "dark";
  interface_language: "ko" | "en";
  history_retention: string;
  per_app_tone: boolean;
  custom_instructions: string;
  dev_mode: boolean;
  tech_stack: string;
  typing_wpm: number;
  compose_factor: number;
  manage_fn_key: boolean;
  onboarding_done: boolean;
}

export interface HistoryEntry {
  id: string;
  created_at: number;
  mode: "dictate" | "translate" | "ask";
  app_name: string;
  bundle_id: string;
  raw_text: string;
  output: string;
  audio_path: string | null;
  duration_ms: number;
  words: number;
  language: string;
  status: "ok" | "error" | "empty";
  error: string | null;
  stt_ms: number;
  llm_ms: number;
  context_json: string;
  action: string;
}

export interface ModelDownload {
  id: string;
  state: "connecting" | "downloading" | "retrying" | "verifying" | "paused" | "error" | "done" | "cancelled";
  downloaded: number;
  total: number;
  bytes_per_sec: number;
  error: string | null;
  attempt: number;
}
export interface LocalModel {
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
  download: ModelDownload | null;
}

export interface DictWord {
  id: number;
  term: string;
  source: "manual" | "auto";
  created_at: number;
}

export interface DayStat {
  day: string;
  words: number;
  audio_ms: number;
  sessions: number;
}
export interface Stats {
  total_words: number;
  total_audio_ms: number;
  avg_wpm: number;
  time_saved_ms: number;
  active_days: number;
  current_streak: number;
  longest_streak: number;
  days: DayStat[];
}

export interface PermState {
  accessibility: boolean;
  microphone: "granted" | "denied" | "unknown";
  fn_usage_type: number | null;
  hotkeys_ready: boolean;
  fn_managed: boolean;
  typeless_running: boolean;
}

export interface HudState {
  phase: "recording" | "listening" | "transcribing" | "thinking" | "processing" | "message" | "hidden";
  mode: "dictate" | "translate" | "ask" | "";
  hands_free: boolean;
  started_at: number;
  target: TranslationTarget | null;
  targets: TranslationTarget[];
  target_idx: number;
  message: string | null;
  tone: "info" | "error" | "success" | "";
  countdown: number | null;
}

export interface CardContent {
  kind: "answer" | "result";
  title: string;
  text: string;
  question: string;
  copied: boolean;
}


type Res<T> = { Ok: T } | { Err: string };
export interface KeyCheck {
  elevenlabs: Res<string>;
  openrouter: Res<string>;
}

export const api = {
  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<Settings>("save_settings", { settings }),
  resetApiKey: (which: "elevenlabs" | "openrouter") => invoke<Settings>("reset_api_key", { which }),
  testApiKeys: (elevenlabs: string, openrouter: string) => invoke<KeyCheck>("test_api_keys", { elevenlabs, openrouter }),
  getPermissions: () => invoke<PermState>("get_permissions"),
  requestAccessibility: () => invoke<void>("request_accessibility"),
  openPrivacyPane: (kind: "accessibility" | "microphone" | "input" | "keyboard") => invoke<void>("open_privacy_pane", { kind }),
  listMicrophones: () => invoke<string[]>("list_microphones"),
  startMicTest: (device: string | null) => invoke<void>("start_mic_test", { device }),
  stopMicTest: () => invoke<void>("stop_mic_test"),
  listHistory: (filter: string, query: string, limit = 200, offset = 0) =>
    invoke<HistoryEntry[]>("list_history", { filter, query, limit, offset }),
  deleteHistory: (id: string) => invoke<void>("delete_history", { id }),
  deleteAllHistory: () => invoke<void>("delete_all_history"),
  retryHistory: (id: string) => invoke<HistoryEntry>("retry_history", { id }),
  exportAudio: (id: string, dest: string) => invoke<void>("export_audio", { id, dest }),
  copyText: (text: string) => invoke<void>("copy_text", { text }),
  listDictionary: () => invoke<DictWord[]>("list_dictionary"),
  addWords: (terms: string[]) => invoke<number>("add_words", { terms }),
  updateWord: (id: number, term: string) => invoke<void>("update_word", { id, term }),
  deleteWords: (ids: number[]) => invoke<void>("delete_words", { ids }),
  importDictionary: (path: string) => invoke<number>("import_dictionary", { path }),
  getStats: () => invoke<Stats>("get_stats"),
  localModels: () => invoke<LocalModel[]>("local_models"),
  downloadLocalModel: (id: string) => invoke<void>("download_local_model", { id }),
  pauseLocalDownload: (id: string) => invoke<void>("pause_local_download", { id }),
  cancelLocalDownload: (id: string) => invoke<void>("cancel_local_download", { id }),
  deleteLocalModel: (id: string) => invoke<void>("delete_local_model", { id }),
  hudStop: () => invoke<void>("hud_stop"),
  hudCancel: () => invoke<void>("hud_cancel"),
  hudSetTarget: (idx: number) => invoke<void>("hud_set_target", { idx }),
  cardClose: () => invoke<void>("card_close"),
  recordShortcutStart: () => invoke<void>("record_shortcut_start"),
  recordShortcutCancel: () => invoke<void>("record_shortcut_cancel"),
  appInfo: () =>
    invoke<{
      version: string;
      data_dir: string;
      config_path: string;
      has_default_keys: boolean;
      platform: string;
      default_shortcuts: Settings["shortcuts"];
    }>("app_info"),
  processTextPreview: (text: string, mode: "dictate" | "translate") =>
    invoke<string>("process_text_preview", { text, mode }),
};

export function isOk<T>(r: Res<T>): r is { Ok: T } {
  return "Ok" in r;
}

const KEY_LABELS: Record<string, string> = {
  Fn: "Fn",
  ShiftLeft: "Left Shift",
  ShiftRight: "Right Shift",
  ControlLeft: "Left Ctrl",
  ControlRight: "Right Ctrl",
  AltLeft: isWin ? "Left Alt" : "Left Option",
  AltRight: isWin ? "Right Alt" : "Right Option",
  MetaLeft: isWin ? "Win" : "Left ⌘",
  MetaRight: isWin ? "Right Win" : "Right ⌘",
  Lang1: "한/영",
  Lang2: "한자",
  Space: "Space",
  Escape: "Esc",
  Enter: "Return",
  Backspace: "Delete",
  MouseMiddle: "Middle click",
  Mouse4: "Mouse back",
  Mouse5: "Mouse forward",
  ArrowLeft: "←",
  ArrowRight: "→",
  ArrowUp: "↑",
  ArrowDown: "↓",
};

const ORDER = ["Fn", "ControlLeft", "ControlRight", "AltLeft", "AltRight", "ShiftLeft", "ShiftRight", "MetaLeft", "MetaRight"];

export function keyLabel(k: string): string {
  if (KEY_LABELS[k]) return KEY_LABELS[k];
  if (k.startsWith("Key")) return k.slice(3);
  if (k.startsWith("Digit")) return k.slice(5);
  return k;
}

export function sortKeys(keys: string[]): string[] {
  return [...keys].sort((a, b) => {
    const ia = ORDER.indexOf(a);
    const ib = ORDER.indexOf(b);
    return (ia < 0 ? 99 : ia) - (ib < 0 ? 99 : ib);
  });
}

/** "Ctrl + Win", "Fn + Space"… for hints. */
export function comboLabel(c: Shortcut | undefined): string {
  return c ? sortKeys(c).map(keyLabel).join(" + ") : "";
}
