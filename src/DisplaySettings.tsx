import { useEffect, useState } from "react";
import { core, errorMessage, native, type DisplayScale } from "./core";

const targets = [{ key: "petPercent", label: "角色缩放" }, { key: "panelPercent", label: "面板缩放" }] as const;

export function DisplaySettings() {
  const [scales, setScales] = useState<DisplayScale>({ petPercent: 100, panelPercent: 100 });
  const [draft, setDraft] = useState(scales);
  const [ready, setReady] = useState(!native);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [saved, setSaved] = useState("");
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
