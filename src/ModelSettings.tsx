import { useEffect, useState } from "react";
import { core, errorMessage, native, type ModelCatalog } from "./core";

export function ModelSettings() {
  const [catalog, setCatalog] = useState<ModelCatalog | null>(null);
  const [draft, setDraft] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [message, setMessage] = useState("");
  useEffect(() => {
    if (!native) return;
    let disposed = false;
    void core.live2dCatalog().then((value) => { if (!disposed) { setCatalog(value); setDraft(value.selected.id); } })
      .catch((e) => { if (!disposed) setError(errorMessage(e)); });
    return () => { disposed = true; };
  }, []);
  return <section className="display-settings" aria-labelledby="live2d-title">
    <h1 id="live2d-title">Live2D 角色</h1>
    <p>选择模型 JSON 导入，需保留同目录下的纹理、动作等资源。导入后可移动原文件夹。</p>
    <fieldset disabled={!native || busy || !catalog}>
      <label htmlFor="live2d-model">角色模型</label>
      <select id="live2d-model" value={draft} onChange={(e) => { setDraft(e.target.value); setMessage(""); }}>
        {catalog?.models.map((model) => <option key={model.id} value={model.id}>{model.name}</option>)}
      </select>
      <div className="model-actions">
        <button className="primary" disabled={!catalog || draft === catalog.selected.id} onClick={async () => {
          setBusy(true); setError(""); setMessage("");
          try {
            const selected = await core.selectLive2d(draft);
            setCatalog((old) => old ? { ...old, selected } : old);
            setMessage("角色选择已保存，下次启动继续使用。加载失败时可切回内置角色。");
          } catch (e) { setError(errorMessage(e)); }
          finally { setBusy(false); }
        }}>切换角色</button>
        <button className="primary" onClick={async () => {
          setBusy(true); setError(""); setMessage("");
          try {
            const entry = await core.importLive2d();
            if (entry) {
              setCatalog(await core.live2dCatalog()); setDraft(entry.id);
              setMessage(`已导入 ${entry.name}，点击“切换角色”使用。`);
            }
          } catch (e) { setError(errorMessage(e)); }
          finally { setBusy(false); }
        }}>{busy ? "处理中…" : "导入模型"}</button>
      </div>
    </fieldset>
    {message && <p role="status">{message}</p>}
    {error && <p className="error" role="alert">{error}</p>}
  </section>;
}
