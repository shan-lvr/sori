import { createContext, ReactNode, useCallback, useContext, useRef, useState } from "react";

type Tone = "success" | "error" | "info";
type Action = { label: string; onClick: () => void };
type Toast = { id: number; msg: string; tone: Tone; action?: Action };
type Show = (msg: string, opts?: { tone?: Tone; action?: Action; ms?: number }) => void;

const ToastContext = createContext<Show>(() => {});

export function useToast() {
  return useContext(ToastContext);
}

export function ToastProvider({ children }: { children: ReactNode }) {
  const [toasts, setToasts] = useState<Toast[]>([]);
  const next = useRef(1);
  const show = useCallback<Show>((msg, opts) => {
    const id = next.current++;
    const t: Toast = { id, msg, tone: opts?.tone ?? "success", action: opts?.action };
    setToasts((prev) => [...prev.slice(-2), t]);
    setTimeout(() => setToasts((prev) => prev.filter((x) => x.id !== id)), opts?.ms ?? (t.tone === "error" ? 5000 : 2400));
  }, []);
  return (
    <ToastContext.Provider value={show}>
      {children}
      <div className="toasts" role="status" aria-live="polite">
        {toasts.map((t) => (
          <div key={t.id} className={`toast ${t.tone}`}>
            <span className="toast-icon">{t.tone === "success" ? "✓" : t.tone === "error" ? "!" : "i"}</span>
            <span className="toast-msg">{t.msg}</span>
            {t.action && (
              <button
                className="toast-action"
                onClick={() => {
                  t.action!.onClick();
                  setToasts((prev) => prev.filter((x) => x.id !== t.id));
                }}
              >
                {t.action.label}
              </button>
            )}
          </div>
        ))}
      </div>
    </ToastContext.Provider>
  );
}
