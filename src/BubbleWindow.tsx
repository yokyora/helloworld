import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { errorMessage } from "./core";
import "./App.css";

interface BubbleContent { revision: number; text: string; reply: boolean; left: boolean }
export function BubbleWindow() {
  const [content, setContent] = useState<BubbleContent | null>(null);
  const [error, setError] = useState("");
  useEffect(() => {
    let disposed = false;
    let stop: (() => void) | undefined;
    void (async () => {
      const unlisten = await listen<BubbleContent>("bubble-content", ({ payload }) => {
        if (!disposed) setContent((old) => !old || payload.revision >= old.revision ? payload : old);
      });
      if (disposed) { unlisten(); return; }
      stop = unlisten;
      await invoke("bubble_ready");
    })().catch((e) => { if (!disposed) setError(errorMessage(e)); });
    return () => { disposed = true; stop?.(); };
  }, []);
  if (!content && !error) return null;
  return <main className={`status-bubble ${content?.left ? "bubble-left" : "bubble-right"} ${content?.reply ? "bubble-reply" : ""}`} role="status">
    <p>{error || content?.text}</p>
    {content?.reply && <small>完整内容见聊天面板</small>}
  </main>;
}
