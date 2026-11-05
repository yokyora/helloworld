import { useEffect, useState } from "react";
import { core, errorMessage, native, type DisplayScale } from "./core";
import { listen } from "@tauri-apps/api/event";
import type { BubbleCalibration } from "./core";

const targets = [{ key: "petPercent", label: "角色缩放" }, { key: "panelPercent", label: "面板缩放" }] as const;

export function DisplaySettings() {
  const [scales, setScales] = useState<DisplayScale>({ petPercent: 100, panelPercent: 100 });
  const [draft, setDraft] = useState(scales);
  const [ready, setReady] = useState(!native);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [saved, setSaved] = useState("");
  const [calibration, setCalibration] = useState<BubbleCalibration | null>(null);
  useEffect(() => {
    if (!native) return;
    let disposed = false;
    let stop: (() => void) | undefined;
    void (async () => {
      const unlisten = await listen<BubbleCalibration>("bubble-calibration", ({ payload }) => { if (!disposed) setCalibration(payload); });
      if (disposed) { unlisten(); return; }
      stop = unlisten;
      const state = await core.bubbleCalibration();
      if (!disposed) setCalibration(state);
    })().catch((e) => { if (!disposed) setError(errorMessage(e)); });
    return () => { disposed = true; stop?.(); };
  }, []);
  useEffect(() => {
    if (!native) return;
    let disposed = false;
    void core.displayScale().then((value) => {
      if (!disposed) { setScales(value); setDraft(value); setReady(true); }
    }).catch((e) => { if (!disposed) setError(errorMessage(e)); });
    return () => { disposed = true; };
  }, []);
  return <section className="display-settings" aria-labelledby="display-title">
    <h1 id="display-title">显示</h1>
    <p>在系统缩放基础上，分别调整角色和面板的大小。</p>
    <p>{!calibration?.modelReady ? "角色加载完成后可校准气泡。" : calibration.calibrated ? "当前模型的气泡位置已就绪，可重新校准。" : "当前模型尚未校准，请先设置气泡位置。"}</p>
    <button className="primary" disabled={!native || !calibration?.modelReady || busy} onClick={async () => {
      setError("");
      try { await core.startBubbleCalibration(); }
      catch (e) { setError(errorMessage(e)); }
    }}>校准气泡位置</button>
    {targets.map(({ key, label }) => <form key={key} onSubmit={async (event) => {
      event.preventDefault(); if (!ready || busy || !native) return;
      setBusy(true); setSaved(""); setError("");
      try { setScales(await core.setDisplayScale({ ...scales, [key]: draft[key] })); setSaved(label); }
      catch (e) { setError(errorMessage(e)); }
      finally { setBusy(false); }
    }}>
      <fieldset disabled={!native || !ready || busy}>
        <label htmlFor={`display-${key}`}>{label}</label>
        <select id={`display-${key}`} value={draft[key]} onChange={(event) => { setDraft({ ...draft, [key]: Number(event.target.value) }); setSaved(""); }}>
          {[75, 100, 125, 150, 175, 200].map((value) => <option key={value} value={value}>{value}%{value === 100 ? "（默认）" : ""}</option>)}
        </select>
        <button className="primary" type="submit" disabled={draft[key] === scales[key]}>{`应用${label}`}</button>
      </fieldset>
    </form>)}
    {saved && <p role="status">{saved}已保存，下次启动继续使用。</p>}
    {error && <p className="error" role="alert">{error}</p>}
  </section>;
}
