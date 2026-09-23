import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import "./styles.css";
import MainApp from "./main/App";
import Hud from "./overlay/Hud";
import Card from "./overlay/Card";

function label(): string {
  try {
    return getCurrentWebviewWindow().label;
  } catch {
    // Plain browser (UI development without Tauri).
    return new URLSearchParams(location.search).get("window") ?? "main";
  }
}

const which = label();
if (which !== "main") document.body.classList.add("overlay");

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>{which === "hud" ? <Hud /> : which === "card" ? <Card /> : <MainApp />}</React.StrictMode>,
);
