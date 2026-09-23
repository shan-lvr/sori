import { useEffect, useRef, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { on as listen } from "../bridge";
import { api, ClaudeStatus } from "../api";
import { useL } from "../i18n";
import { useToast } from "./toast";

const errText = (e: unknown) => (typeof e === "string" ? e : e instanceof Error ? e.message : String(e));

export function useClaudeStatus() {
  const [st, setSt] = useState<ClaudeStatus | null>(null);
  const refresh = () =>
    api
      .claudeStatus()
      .then(setSt)
      .catch(() => {});
  useEffect(() => {
    refresh();
    window.addEventListener("focus", refresh);
    window.addEventListener("claude-refresh", refresh);
    return () => {
      window.removeEventListener("focus", refresh);
      window.removeEventListener("claude-refresh", refresh);
    };
  }, []);
  // Install / sign-in may be driven from elsewhere (Home's "Set up everything"): follow along.
  const active = !!st?.installing || !!st?.logging_in;
  useEffect(() => {
    if (!active) return;
    const iv = setInterval(refresh, 1500);
    return () => clearInterval(iv);
  }, [active]);
  return [st, refresh, setSt] as const;
}

/** Install → sign in → test, each one click. Used in Settings → AI and the Home setup list. */
export default function ClaudeSetup({ compact = false }: { compact?: boolean }) {
  const L = useL();
  const toast = useToast();
  const [st, refresh, setSt] = useClaudeStatus();
  const [busy, setBusy] = useState<"install" | "login" | "test" | null>(null);
  const [log, setLog] = useState<string[]>([]);
  const [url, setUrl] = useState<string | null>(null);
  const [code, setCode] = useState("");
  const [test, setTest] = useState<{ ok: boolean; text: string } | null>(null);
  const logRef = useRef<HTMLPreElement>(null);

  useEffect(() => {
    const uns = [
      listen<string>("claude-log", (e) => setLog((l) => [...l.slice(-200), e.payload])),
      listen<string>("claude-login-url", (e) => setUrl(e.payload)),
    ];
    return () => uns.forEach((p) => p.then((u) => u()));
  }, []);
  useEffect(() => {
    logRef.current?.scrollTo({ top: logRef.current.scrollHeight });
  }, [log]);

  const install = async () => {
    setBusy("install");
    setLog([]);
    try {
      setSt(await api.claudeInstall());
      toast(L("Claude Code CLI installed", "Claude Code CLI를 설치했어요"));
    } catch (e) {
      toast(`${L("Install failed", "설치 실패")}: ${errText(e)}`, { tone: "error", ms: 6000 });
    } finally {
      setBusy(null);
      refresh();
    }
  };

  const login = async () => {
    setBusy("login");
    setLog([]);
    setUrl(null);
    setCode("");
    try {
      const s = await api.claudeLogin();
      setSt(s);
      if (s.logged_in) toast(L("Signed in to Claude", "Claude에 로그인했어요"));
    } catch (e) {
      const msg = errText(e);
      if (!/cancel/i.test(msg)) toast(`${L("Sign-in failed", "로그인 실패")}: ${msg}`, { tone: "error", ms: 6000 });
    } finally {
      setBusy(null);
      refresh();
    }
  };

  const submitCode = async () => {
    if (!code.trim()) return;
    try {
      await api.claudeLoginCode(code.trim());
      setCode("");
    } catch (e) {
      toast(errText(e), { tone: "error" });
    }
  };

  const runTest = async () => {
    setBusy("test");
    setTest(null);
    try {
      setTest({ ok: true, text: await api.claudeTest() });
    } catch (e) {
      setTest({ ok: false, text: errText(e) });
    } finally {
      setBusy(null);
    }
  };

  if (!st) return <span className="hint">{L("Checking Claude Code…", "Claude Code 확인 중…")}</span>;

  const logBox = log.length > 0 && (busy !== null || st.installing || st.logging_in || !compact) && (
    <pre ref={logRef} className="cli-log selectable">
      {log.join("\n")}
    </pre>
  );

  const installing = busy === "install" || st.installing;
  const loggingIn = busy === "login" || st.logging_in;

  if (!st.installed || installing) {
    return (
      <div className="claude-setup">
        {!compact && (
          <p className="hint">
            {L(
              "Sori runs cleanup through the Claude Code CLI on your own Claude plan. The desktop app alone isn't enough — install the CLI (no admin password needed).",
              "Sori는 Claude Code CLI로 내 Claude 요금제에서 다듬기를 실행해요. 데스크톱 앱만으로는 안 되고 CLI가 필요해요 (관리자 암호 필요 없음).",
            )}
          </p>
        )}
        <button className="btn primary small" disabled={busy !== null || installing} onClick={install}>
          {installing ? L("Installing…", "설치 중…") : L("Install Claude Code CLI", "Claude Code CLI 설치")}
        </button>
        {logBox}
      </div>
    );
  }

  if (!st.logged_in || loggingIn) {
    return (
      <div className="claude-setup">
        {!compact && (
          <span className="key-status ok">
            ✓ {L("CLI installed", "CLI 설치됨")} {st.version ? `· v${st.version}` : ""}
          </span>
        )}
        {loggingIn ? (
          <>
            <p className="hint">
              {L(
                "A browser window opened — approve access there. Sori finishes automatically when you're done.",
                "브라우저 창에서 접근을 허용하세요. 끝나면 Sori가 자동으로 마무리해요.",
              )}
            </p>
            {url && (
              <button className="btn small ghost" onClick={() => openUrl(url)}>
                {L("Browser didn't open? Open the sign-in page", "브라우저가 안 열렸나요? 로그인 페이지 열기")}
              </button>
            )}
            <div className="key-field">
              <input
                className="input"
                placeholder={L("If the page shows a code, paste it here", "페이지에 코드가 보이면 여기에 붙여넣기")}
                value={code}
                onChange={(e) => setCode(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && submitCode()}
                spellCheck={false}
              />
              <button className="btn small" disabled={!code.trim()} onClick={submitCode}>
                {L("Submit", "제출")}
              </button>
            </div>
            <button className="btn small ghost" style={{ justifySelf: "start" }} onClick={() => api.claudeLoginCancel()}>
              {L("Cancel", "취소")}
            </button>
          </>
        ) : (
          <button className="btn primary small" onClick={login}>
            {L("Sign in with Claude", "Claude로 로그인")}
          </button>
        )}
        {logBox}
      </div>
    );
  }

  return (
    <div className="claude-setup">
      <span className="key-status ok">
        ✓ {L("Signed in", "로그인됨")}
        {st.auth_method && st.auth_method !== "none" ? ` (${st.auth_method})` : ""} {st.version ? `· Claude Code v${st.version}` : ""}
      </span>
      {!compact && (
        <button className="btn small" disabled={busy !== null} onClick={runTest}>
          {busy === "test" ? L("Testing…", "테스트 중…") : L("Test cleanup", "다듬기 테스트")}
        </button>
      )}
      {test && <span className={`key-status ${test.ok ? "ok" : "bad"}`}>{test.ok ? `✓ ${test.text}` : `✕ ${test.text}`}</span>}
    </div>
  );
}
