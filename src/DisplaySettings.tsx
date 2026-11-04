import { useEffect, useState } from "react";
import { core, errorMessage, native } from "./core";

export function DisplaySettings() {
  const [percent, setPercent] = useState(100);
  const [ready, setReady] = useState(!native);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [saved, setSaved] = useState(false);
  useEffect(() => {
    if (!native) return;
    let disposed = false;
    void core.displayScale().then((value) => {
      if (!disposed) { setPercent(value); setReady(true); }
    }).catch((e) => { if (!disposed) setError(errorMessage(e)); });
    return () => { disposed = true; };
  }, []);
  return <section className="display-settings" aria-labelledby="display-title">
    <h1 id="display-title">显示</h1>
    <p>在系统缩放基础上，调整角色和面板的整体大小。</p>
    <form onSubmit={async (event) => {
      event.preventDefault(); if (!ready || busy || !native) return;
      setBusy(true); setSaved(false); setError("");
      try { setPercent(await core.setDisplayScale(percent)); setSaved(true); }
      catch (e) { setError(errorMessage(e)); }
      finally { setBusy(false); }
    }}>
      <fieldset disabled={!native || !ready || busy}>
        <label htmlFor="display-scale">界面缩放</label>
        <select id="display-scale" value={percent} onChange={(event) => { setPercent(Number(event.target.value)); setSaved(false); }}>
          {[75, 100, 125, 150, 175, 200].map((value) => <option key={value} value={value}>{value}%{value === 100 ? "（默认）" : ""}</option>)}
        </select>
        <button className="primary" type="submit">{busy ? "正在应用…" : "应用缩放"}</button>
      </fieldset>
    </form>
    {saved && <p role="status">缩放已保存，下次启动继续使用。</p>}
    {error && <p className="error" role="alert">{error}</p>}
  </section>;
}
