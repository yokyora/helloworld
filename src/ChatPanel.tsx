import { useEffect, useRef, useState } from "react";
import { native, type Settings, type Turn } from "./core";

interface Props {
  turns: Turn[]; settings: Settings; sending: boolean; unavailable: boolean; resting: boolean;
  onSend: (text: string) => Promise<boolean>; onCancel: () => Promise<void>; onClear: () => Promise<boolean>; onSettings: () => void;
}
export function ChatPanel({ turns, settings, sending, unavailable, resting, onSend, onCancel, onClear, onSettings }: Props) {
  const [draft, setDraft] = useState("");
  const [confirmClear, setConfirmClear] = useState(false);
  const list = useRef<HTMLDivElement>(null);
  const follow = useRef(true);
  useEffect(() => { if (follow.current && list.current) list.current.scrollTop = list.current.scrollHeight; }, [turns]);
  const configured = native && settings.hasApiKey && settings.allowRemote;
  return <section id="chat-panel" className="detail-panel chat-panel" aria-labelledby="chat-title">
    <div className="panel-heading"><h1 id="chat-title">和我聊聊</h1>
      <button className="text-button" disabled={sending || unavailable || turns.length === 0} onClick={() => setConfirmClear(true)}>清空对话</button></div>
    {!configured && <p>{native ? "先配置密钥，并确认允许把对话发送给 DeepSeek。" : "浏览器仅预览界面；请打开桌面程序聊天。"}<button className="text-button" onClick={onSettings}>打开设置</button></p>}
    <div className="chat-history" ref={list} role="log" aria-label="对话记录" aria-live="off"
      onScroll={() => { const el = list.current; if (el) follow.current = el.scrollHeight - el.scrollTop - el.clientHeight < 45; }}>
      {turns.length === 0 && <p className="empty-chat">还没有对话。聊聊今天发生的小事吧。<br />长期记忆尚未接入。</p>}
      {turns.map((turn) => <article className="turn" key={turn.id}>
        <p className="message user-message"><span className="speaker">你</span>{turn.userText}</p>
        <p className="message assistant-message"><span className="speaker">Hello World</span>{turn.assistantText || (turn.status === "pending" ? "正在等待 DeepSeek…" : "未收到回复。")}</p>
        {turn.status !== "complete" && <span className="turn-status">{turn.status === "pending" ? "正在回复" : turn.status === "cancelled" ? "已停止 · 不纳入后续上下文" : "未完成 · 不纳入后续上下文"}</span>}
      </article>)}
    </div>
    {confirmClear ? <div className="confirm-clear" role="alert">
      <p>清空本机保存的全部对话？此操作不可撤销，不能删除 DeepSeek 服务端可能保留的数据。</p>
      <button disabled={sending || unavailable} onClick={async () => { if (await onClear()) setConfirmClear(false); }}>确认清空</button>
      <button onClick={() => setConfirmClear(false)}>保留</button>
    </div> : <form className="chat-compose" onSubmit={(event) => { event.preventDefault(); if (!draft.trim() || sending || unavailable || !configured || resting) return; follow.current = true; void onSend(draft.trim()).then((ok) => { if (ok) setDraft(""); }); }}>
      <label className="sr-only" htmlFor="chat-input">消息内容</label>
      <textarea id="chat-input" value={draft} maxLength={4000} rows={2} disabled={unavailable || !configured || sending || resting}
        placeholder={resting ? "先叫醒桌宠" : "输入消息，点击发送"} onChange={(e) => setDraft(e.target.value)} />
      <div className="compose-actions"><span>{sending ? "停止后，服务端已处理部分仍可能计费。" : "发送内容及近期对话至 DeepSeek"}</span>
        {sending ? <button type="button" onClick={() => void onCancel()}>停止回复</button> : <button className="primary" disabled={!configured || unavailable || resting || !draft.trim()} type="submit">发送</button>}</div>
    </form>}
  </section>;
}
