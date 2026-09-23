import { useEffect, useMemo, useState } from "react";
import { inTauri, on as listen, PASTE } from "../bridge";
import { openUrl } from "@tauri-apps/plugin-opener";
import { marked } from "marked";
import { api, CardContent, Settings } from "../api";
import { IconCopy, IconX } from "../icons";
import { useTheme } from "../main/App";
import { makeL, useOverlayLang } from "../i18n";

/** Render LLM Markdown without letting it inject active HTML into a privileged webview. */
function renderMarkdown(md: string): string {
  const escaped = md.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
  const html = marked.parse(escaped, { async: false, gfm: true, breaks: true }) as string;
  const doc = new DOMParser().parseFromString(`<div>${html}</div>`, "text/html");
  doc.querySelectorAll("script,style,iframe,object,embed,link,meta,form,img").forEach((n) => n.remove());
  doc.querySelectorAll("*").forEach((el) => {
    for (const a of Array.from(el.attributes)) {
      const v = a.value.trim().toLowerCase();
      if (a.name.startsWith("on") || ((a.name === "href" || a.name === "src") && !/^https?:/.test(v))) el.removeAttribute(a.name);
    }
  });
  return doc.body.firstElementChild?.innerHTML ?? "";
}

export default function Card() {
  const [c, setC] = useState<CardContent | null>(null);
  const [copied, setCopied] = useState(false);
  const [settings, setSettings] = useState<Settings | null>(null);
  useTheme(settings?.appearance);
  const lang = useOverlayLang();
  const L = makeL(lang);

  useEffect(() => {
    api.getSettings().then(setSettings);
    if (!inTauri) {
      setC({
        kind: "answer",
        title: "Ask anything",
        question: "Summarize this paragraph in three lines",
        copied: true,
        text: "**Summary**\n\n1. Typeless is a dictation app that polishes what you say and pastes it in.\n2. It offers Translate and Ask Anything modes.\n3. History is stored only on your device.\n\n[Source](https://www.typeless.com) <script>alert(1)</script>",
      });
    }
    const uns = [
      listen<CardContent>("card-content", (e) => {
        setC(e.payload);
        setCopied(false);
      }),
      listen<Settings>("settings-changed", (e) => setSettings(e.payload)),
    ];
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") api.cardClose();
    };
    window.addEventListener("keydown", onKey);
    return () => {
      uns.forEach((p) => p.then((u) => u()));
      window.removeEventListener("keydown", onKey);
    };
  }, []);

  const html = useMemo(() => (c ? renderMarkdown(c.text) : ""), [c]);
  if (!c) return <div className="card-root" />;

  return (
    <div className="card-root">
      <div className="answer">
        <div className="answer-head" data-tauri-drag-region>
          <span className="title">{c.title}</span>
          <span style={{ flex: 1 }} />
          <button className="icon-btn" title={L("Close (Esc)", "닫기 (Esc)")} onClick={() => api.cardClose()}>
            <IconX />
          </button>
        </div>
        {c.question && <div className="answer-q">“{c.question}”</div>}
        {c.kind === "result" && (
          <div className="answer-q">
            {c.copied
              ? L(
                  `Couldn't find a text field, so it's copied to your clipboard. Paste it anywhere with ${PASTE}.`,
                  `텍스트 입력 칸을 찾지 못해 클립보드에 복사해 두었어요. 원하는 곳에 ${PASTE}로 붙여넣으세요.`,
                )
              : L(
                  "Couldn't find a text field. Use the Copy button below, then paste.",
                  "텍스트 입력 칸을 찾지 못했어요. 아래 복사 버튼으로 복사해 붙여넣으세요.",
                )}
          </div>
        )}
        <div
          className="answer-body selectable"
          dangerouslySetInnerHTML={{ __html: html }}
          onClick={(e) => {
            const a = (e.target as HTMLElement).closest("a");
            if (a?.href) {
              e.preventDefault();
              openUrl(a.href);
            }
          }}
        />
        <div className="answer-foot">
          <button
            className="btn"
            onClick={async () => {
              await api.copyText(c.text);
              setCopied(true);
            }}
          >
            <span style={{ display: "inline-flex", width: 14, height: 14, marginRight: 6, verticalAlign: -2 }}>
              <IconCopy />
            </span>
            {copied || c.copied ? L("Copied", "복사됨") : L("Copy", "복사")}
          </button>
          <button className="btn primary" onClick={() => api.cardClose()}>
            {L("Close", "닫기")}
          </button>
        </div>
      </div>
    </div>
  );
}
