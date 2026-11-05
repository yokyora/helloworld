import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { errorMessage } from "./core";
import "./App.css";

interface BubbleContent { revision: number; text: string; reply: boolean; left: boolean }
export function BubbleWindow() {
  const [content, setContent] = useState<BubbleContent | null>(null);
  const [error, setError] = useState("");
  const frame = useRef<HTMLElement>(null);
  const [size, setSize] = useState({ width: 118, height: 52 });
  const visible = Boolean(content || error);
  useEffect(() => {
    if (!visible || !frame.current) return;
    const element = frame.current;
    const update = () => setSize({ width: element.clientWidth, height: element.clientHeight });
    const observer = new ResizeObserver(update);
    observer.observe(element); update();
    return () => observer.disconnect();
  }, [visible]);
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
  const right = size.width - 9, bottom = size.height - 12;
  const outline = `M23 3 H${right - 14} Q${right} 3 ${right} 17 V${bottom - 14} Q${right} ${bottom} ${right - 14} ${bottom} H15 Q9 ${bottom} 9 ${bottom - 6} L1 ${bottom - 8} L9 ${bottom - 14} V17 Q9 3 23 3 Z`;
  return <main ref={frame} className={`status-bubble ${content?.reply ? "bubble-reply" : ""}`} role="status">
    <svg className="bubble-outline" width="100%" height="100%" aria-hidden="true">
      <path d={outline} transform={content?.left ? `translate(${size.width} 0) scale(-1 1)` : undefined} />
    </svg>
    <p>{error || content?.text}</p>
    {content?.reply && <small>完整内容见聊天面板</small>}
  </main>;
}
