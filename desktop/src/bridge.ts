// Thin wrapper over Tauri IPC so the UI can also run in a plain browser (vite dev) with
// mock data — handy for layout work without rebuilding the app.
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen, EventCallback, UnlistenFn } from "@tauri-apps/api/event";

export const inTauri = isTauri();
/** Platform-specific wording/keys (the WebView2 user agent says "Windows NT"). */
export const isWin = typeof navigator !== "undefined" && /Windows/.test(navigator.userAgent);
export const PASTE = isWin ? "Ctrl+V" : "⌘V";
export const DEVICE = isWin ? "PC" : "Mac";

export async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (inTauri) return invoke<T>(cmd, args);
  const { mockInvoke } = await import("./mock");
  return (await mockInvoke(cmd, args)) as T;
}

/** Native yes/no dialog (window.confirm doesn't work inside the app's WKWebView). */
export async function confirmDialog(message: string, okLabel?: string): Promise<boolean> {
  const ko = document.documentElement.lang === "ko";
  okLabel ??= ko ? "확인" : "OK";
  if (!inTauri) return window.confirm(message);
  const { ask } = await import("@tauri-apps/plugin-dialog");
  return ask(message, { title: "Sori", kind: "warning", okLabel, cancelLabel: ko ? "취소" : "Cancel" });
}

export function on<T>(event: string, cb: EventCallback<T>): Promise<UnlistenFn> {
  if (inTauri) return listen<T>(event, cb);
  const handler = (e: Event) => cb({ event, id: 0, payload: (e as CustomEvent).detail } as never);
  window.addEventListener(`mock:${event}`, handler);
  return Promise.resolve(() => window.removeEventListener(`mock:${event}`, handler));
}
