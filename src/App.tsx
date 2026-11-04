import { useEffect, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { core, errorMessage, native, type ShellView } from "./core";
import { listen } from "@tauri-apps/api/event";
import { useCompanion } from "./useCompanion";
import { ChatPanel } from "./ChatPanel";
import { SettingsPanel } from "./SettingsPanel";

import "./App.css";

type View = ShellView;

function App() {
  const app = useCompanion();
  const { resting, quiet } = app.snapshot.pet;
  const [view, setView] = useState<View>("menu");
  const [resizing, setResizing] = useState(false);
  const transition = useRef(false);
  const surface = useRef<HTMLElement>(null);
  const expanded = view !== "idle" && view !== "menu";
  const status = !app.ready ? "等待本地数据" : resting ? "休息中" : app.sending ? "正在回复" : quiet ? "勿扰中" : "待机中";

  useEffect(() => {
    if (!native) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void (async () => {
      const stop = await listen<View>("shell-view", (event) => { if (!disposed) setView(event.payload); });
      if (disposed) { stop(); return; }
      unlisten = stop;
      await core.panelReady();
    })().catch((error) => { if (!disposed) app.setError(errorMessage(error)); });
    return () => { disposed = true; unlisten?.(); };
  }, [app.setError]);
  useEffect(() => {
    // Do not take keyboard focus at startup. Returning from a panel restores it.
    if (view !== "idle") surface.current?.querySelector<HTMLButtonElement>("button:not(:disabled)")?.focus();
  }, [view]);

  async function show(next: View) {
    if (transition.current || next === view) return;
    transition.current = true; setResizing(true);
    try {
      if (native) await core.shell(next);
      else setView(next);
    } catch (error) { app.setError(errorMessage(error)); }
    finally { transition.current = false; setResizing(false); }
  }
  async function windowAction(action: "startDragging" | "minimize" | "close") {
    try {
      if (action === "startDragging") await getCurrentWindow().startDragging();
      else await core.shellAction(action === "close" ? "exit" : "minimize");
    } catch (error) { app.setError(errorMessage(error)); }
  }
  const controls = <>
    <button disabled={resizing} aria-pressed={view === "chat"} onClick={() => void show("chat")}>聊天</button>
    <button disabled={resizing} aria-pressed={view === "settings"} onClick={() => void show("settings")}>设置</button>
    <button disabled={resizing} aria-pressed={view === "privacy"} onClick={() => void show("privacy")}>隐私</button>
  </>;

  return <main className={`companion detached-panel view-${view} ${expanded ? "panel-open" : ""}`}
    onKeyDown={(event) => { if (event.key === "Escape") { event.preventDefault(); void show("idle"); } }}>
    <section className="shell-surface" ref={surface} hidden={view === "idle"} aria-label={view === "menu" ? "桌宠快捷入口" : "桌宠面板"}>
      <header className="window-bar">
        <button className="drag-handle" aria-label="拖动面板" disabled={!native || resizing}
          onPointerDown={(event) => { if (event.button === 0 && native) void windowAction("startDragging"); }}>
          <span className="brand-mark" aria-hidden="true">✳</span> {status}
        </button>
        <div className="window-actions">
          {native && <button aria-label="最小化到任务栏" onClick={() => void windowAction("minimize")}>−</button>}
          <button aria-label="收起面板，只显示角色" title="收起 · Esc" disabled={resizing} onClick={() => void show("idle")}>⌄</button>
        </div>
      </header>
      <nav className="pet-controls" aria-label="桌宠功能">{controls}</nav>
      {view === "menu" && <>
        <div className="pet-controls secondary-controls">
          <button disabled={!app.ready || app.busy} aria-pressed={resting} onClick={() => void app.setPet({ resting: !resting, quiet })}>{resting ? "叫醒" : "休息"}</button>
          <button disabled={!app.ready || app.busy} aria-pressed={quiet} onClick={() => void app.setPet({ resting, quiet: !quiet })}>{quiet ? "取消勿扰" : "勿扰"}</button>
          <button disabled={!native} onClick={() => void windowAction("close")}>退出</button>
        </div>
        <p className="gesture-hint">拖动角色移动 · 点击角色或 Esc 收起</p>
      </>}
      <div className="detail-area" hidden={!expanded}>
        <div className="chat-container" hidden={view !== "chat"}>
          <ChatPanel turns={app.turns} settings={app.snapshot.settings} sending={app.sending}
            unavailable={!app.ready || app.busy} resting={resting} onSend={app.send} onCancel={app.cancel}
            onClear={app.clear} onSettings={() => void show("settings")} />
        </div>
        {view === "settings" && <SettingsPanel settings={app.snapshot.settings} disabled={app.busy || app.sending} onSave={app.saveSettings} />}
        {view === "privacy" && <section id="privacy-panel" className="detail-panel" aria-labelledby="privacy-title">
          <h1 id="privacy-title">你掌握陪伴的边界</h1>
          <p>仅在你确认授权并点击发送后，向 DeepSeek 发送当前消息及最多 8 轮近期完整对话。不读取屏幕或其他应用，不执行外部操作。</p>
          <p>聊天文本保存在本机，未加密；密钥使用 Windows 当前用户加密保护。长期记忆尚未接入。</p>
          <p>勿扰暂停动画与戳戳反馈，仍可手动聊天。休息仅本次运行生效，并停止当前回复。</p>
          <p>收起面板不会停止正在生成的回复。需要停止时，请回到聊天点击「停止回复」。</p>
          <button className="text-button" onClick={() => void show("settings")}>管理 DeepSeek 授权与密钥</button>
        </section>}
      </div>
      {app.error && <p className="error" role="alert">{app.error}</p>}
      {!app.ready && <button className="text-button" onClick={() => void app.refresh()}>重试读取本地数据</button>}
    </section>
  </main>;
}
export default App;
