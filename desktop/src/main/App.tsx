import { ReactNode, useCallback, useEffect, useState } from "react";
import { on as listen } from "../bridge";
import { api, PermState, Settings } from "../api";
import { LangContext, useL, useT } from "../i18n";
import { IconBook, IconClock, IconGear, IconHome, IconInfo } from "../icons";
import Home from "./Home";
import History from "./History";
import Dictionary from "./Dictionary";
import SettingsModal from "./Settings";
import { DownloadPills } from "./ModelDownload";
import { ToastProvider } from "./toast";

type Page = "home" | "history" | "dictionary";

export function useTheme(appearance: string | undefined) {
  useEffect(() => {
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const apply = () => {
      const dark = appearance === "dark" || ((appearance ?? "system") === "system" && mq.matches);
      document.documentElement.dataset.theme = dark ? "dark" : "light";
    };
    apply();
    mq.addEventListener("change", apply);
    return () => mq.removeEventListener("change", apply);
  }, [appearance]);
}

export default function MainApp() {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [perms, setPerms] = useState<PermState | null>(null);
  const [page, setPage] = useState<Page>("home");
  const [settingsOpen, setSettingsOpen] = useState<string | null>(null);
  const [historyTick, setHistoryTick] = useState(0);

  const refreshPerms = useCallback(() => api.getPermissions().then(setPerms).catch(() => {}), []);

  useEffect(() => {
    api.getSettings().then(setSettings);
    refreshPerms();
    const uns = [
      listen<Settings>("settings-changed", (e) => setSettings(e.payload)),
      listen("permissions-changed", refreshPerms),
      listen("history-updated", () => setHistoryTick((t) => t + 1)),
      listen("open-settings", () => setSettingsOpen("general")),
    ];
    const iv = setInterval(refreshPerms, 2500);
    window.addEventListener("focus", refreshPerms);
    return () => {
      uns.forEach((p) => p.then((u) => u()));
      clearInterval(iv);
      window.removeEventListener("focus", refreshPerms);
    };
  }, [refreshPerms]);

  useTheme(settings?.appearance);
  useEffect(() => {
    // Native dialogs (bridge.confirmDialog) read the UI language from here.
    document.documentElement.lang = settings?.interface_language === "ko" ? "ko" : "en";
  }, [settings?.interface_language]);

  const save = useCallback(async (next: Settings) => {
    setSettings(next);
    const saved = await api.saveSettings(next);
    setSettings(saved);
    return saved;
  }, []);

  if (!settings) return null;

  return (
    <LangContext.Provider value={settings.interface_language}>
      <ToastProvider>
      <div className="drag" data-tauri-drag-region />
      <div className="app">
        <Sidebar page={page} setPage={setPage} perms={perms} settings={settings} openSettings={(t) => setSettingsOpen(t)} />
        <main className="content">
          {page === "home" && (
            <Home settings={settings} perms={perms} save={save} refreshPerms={refreshPerms} historyTick={historyTick} openSettings={setSettingsOpen} />
          )}
          {page === "history" && <History settings={settings} save={save} tick={historyTick} />}
          {page === "dictionary" && <Dictionary />}
        </main>
      </div>
      {settingsOpen && (
        <SettingsModal
          initialTab={settingsOpen}
          settings={settings}
          perms={perms}
          save={save}
          refreshPerms={refreshPerms}
          onClose={() => setSettingsOpen(null)}
        />
      )}
      </ToastProvider>
    </LangContext.Provider>
  );
}

function Sidebar(props: { page: Page; setPage: (p: Page) => void; perms: PermState | null; settings: Settings; openSettings: (tab: string) => void }) {
  const t = useT();
  const L = useL();
  const items: [Page, string, ReactNode][] = [
    ["home", t.home, <IconHome />],
    ["history", t.history, <IconClock />],
    ["dictionary", t.dictionary, <IconBook />],
  ];
  const ready = props.perms?.hotkeys_ready && props.perms?.accessibility;
  return (
    <aside className="sidebar">
      <div className="brand">
        Sori <span className="badge">Beta</span>
      </div>
      {items.map(([id, label, icon]) => (
        <button key={id} className={`nav-item ${props.page === id ? "active" : ""}`} onClick={() => props.setPage(id)}>
          {icon}
          {label}
        </button>
      ))}
      <div className="spacer" />
      <DownloadPills
        models={[
          ...(props.settings.stt_engine === "local" ? [props.settings.local_model] : []),
          ...(props.settings.llm_provider === "local" ? [props.settings.local_llm_model] : []),
        ]}
        onOpen={() => props.openSettings("ai")}
      />
      <div className="status-pill">
        <span className={`dot ${ready ? "ok" : "bad"}`} />
        {ready ? t.hotkeysReady : t.hotkeysOff}
      </div>
      <div className="sidebar-footer">
        <button className="icon-btn" title={t.settings} onClick={() => props.openSettings("general")}>
          <IconGear />
        </button>
        <button className="icon-btn" title={L("About", "정보")} onClick={() => props.openSettings("about")}>
          <IconInfo />
        </button>
      </div>
    </aside>
  );
}
