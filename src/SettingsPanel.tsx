import { useState } from "react";
import { native, type Settings, type SettingsInput } from "./core";

export function SettingsPanel({ settings, disabled, onSave }: { settings: Settings; disabled: boolean; onSave: (input: SettingsInput) => Promise<boolean> }) {
  const [model, setModel] = useState(settings.model);
  const [key, setKey] = useState("");
  const [removeKey, setRemoveKey] = useState(false);
  const [allow, setAllow] = useState(settings.allowRemote);
  const [saved, setSaved] = useState(false);
  return <section id="settings-panel" className="detail-panel" aria-labelledby="settings-title">
    <h1 id="settings-title">连接 DeepSeek</h1>
    <p>仅连接 api.deepseek.com。保存设置不会发送聊天请求。</p>
    {!native && <p>请在桌面程序中配置，浏览器预览不能保存密钥。</p>}
    <form onSubmit={async (event) => {
      event.preventDefault(); setSaved(false);
      const input = { model, apiKey: key || undefined, removeKey, allowRemote: allow };
      setKey("");
      if (await onSave(input)) { setSaved(true); setRemoveKey(false); }
    }} onChange={() => setSaved(false)}>
      <fieldset disabled={disabled || !native}>
        <label htmlFor="model">模型</label>
        <select id="model" value={model} onChange={(e) => setModel(e.target.value)}>
          <option value="deepseek-flash">DeepSeek Flash · 日常对话</option>
          <option value="deepseek-v4-pro">DeepSeek V4 Pro</option>
        </select>
        <label htmlFor="api-key">API Key <span>{settings.hasApiKey ? "已保存，留空保留" : "尚未配置"}</span></label>
        <input id="api-key" type="password" autoComplete="off" spellCheck={false} value={key} maxLength={512}
          disabled={removeKey} placeholder="只在本机设置，不要发到聊天里" onChange={(e) => setKey(e.target.value)} />
        {settings.hasApiKey && <label className="check-line"><input type="checkbox" checked={removeKey} onChange={(e) => { setRemoveKey(e.target.checked); setKey(""); }} />移除已保存的密钥</label>}
        <label className="check-line consent"><input type="checkbox" checked={allow} onChange={(e) => setAllow(e.target.checked)} />允许将我发送的消息及最多 8 轮近期完整对话发送给 DeepSeek，使用可能产生 API 费用。</label>
        <button type="submit" className="primary">保存设置</button>
      </fieldset>
    </form>
    {disabled && <p>请先停止或等待当前操作完成，再修改设置。</p>}
    {saved && <p role="status">设置已保存。可到聊天页开始对话。</p>}
    <p>密钥使用 Windows 当前用户加密保护；聊天记录保存在本机 SQLite，聊天文本未加密。</p>
  </section>;
}
