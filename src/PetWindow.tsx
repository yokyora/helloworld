import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { core, errorMessage, type PetStatus, type ShellView } from "./core";
import { Live2DPet, type PetLoadStatus } from "./Live2DPet";
import "./App.css";

export function PetWindow() {
  const [status, setStatus] = useState<PetStatus | null>(null);
  const [view, setView] = useState<ShellView>("idle");
  const [error, setError] = useState("");
  const [poked, setPoked] = useState(false);
  const [petLoad, setPetLoad] = useState<PetLoadStatus>("loading");
  const [attempt, setAttempt] = useState(0);
  const gesture = useRef<{ x: number; y: number } | null>(null);
  const dragged = useRef(false);
  const transition = useRef(false);
  const pokeTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const resting = status?.pet.resting ?? false;
  const quiet = status?.pet.quiet ?? false;
  const still = !status || resting || quiet;
  const label = !status ? "等待本地数据" : resting ? "休息中" : status.sending ? "正在回复" : quiet ? "勿扰中" : "待机中";

  useEffect(() => {
    let disposed = false;
    const stops: (() => void)[] = [];
    const accept = (next: PetStatus) => {
      if (!disposed) setStatus((old) => !old || next.revision >= old.revision ? next : old);
    };
    void (async () => {
      const stateStop = await listen<PetStatus>("pet-status", (event) => accept(event.payload));
      if (disposed) { stateStop(); return; }
      stops.push(stateStop);
      const viewStop = await listen<ShellView>("shell-view", (event) => { if (!disposed) setView(event.payload); });
      if (disposed) { viewStop(); return; }
      stops.push(viewStop);
      accept(await core.petStatus());
    })().catch((e) => { if (!disposed) setError(errorMessage(e)); });
    return () => { disposed = true; stops.forEach((stop) => stop()); };
  }, []);

  useEffect(() => () => { if (pokeTimer.current !== null) clearTimeout(pokeTimer.current); }, []);
  useEffect(() => {
    if (still) {
      if (pokeTimer.current !== null) clearTimeout(pokeTimer.current);
      setPoked(false);
    }
  }, [still]);

  async function show(next: ShellView) {
    if (transition.current) return;
    transition.current = true;
    try { await core.shell(next); setError(""); }
    catch (e) { setError(errorMessage(e)); }
    finally { transition.current = false; }
  }
  function poke() {
    if (still) return;
    if (pokeTimer.current !== null) clearTimeout(pokeTimer.current);
    setPoked(true);
    pokeTimer.current = setTimeout(() => setPoked(false), 1400);
  }
  async function drag() {
    try { await getCurrentWindow().startDragging(); }
    catch (e) { setError(errorMessage(e)); }
  }

  return <main className="companion" onKeyDown={(event) => {
    if (event.key === "Escape") { event.preventDefault(); void show("idle"); }
  }}>
    {error && <button className="idle-error" onClick={() => void show("menu")}>{error}</button>}
    <section className="pet-space" aria-label="桌宠互动">
      <button className={`pet ${poked ? "is-poked" : ""} ${resting ? "is-resting" : ""}`}
        aria-label={`${label}，${view === "idle" ? "点击打开快捷入口" : "点击收起面板"}，按住拖动可移动`}
        title="点击打开或收起 · 按住拖动" aria-expanded={view !== "idle"}
        onContextMenu={(event) => { event.preventDefault(); void show("menu"); }}
        onPointerDown={(event) => {
          if (event.button !== 0) return;
          dragged.current = false; gesture.current = { x: event.clientX, y: event.clientY };
          event.currentTarget.setPointerCapture(event.pointerId);
        }}
        onPointerMove={(event) => {
          const start = gesture.current;
          if (!start || transition.current || Math.hypot(event.clientX - start.x, event.clientY - start.y) < 6) return;
          gesture.current = null; dragged.current = true;
          if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId);
          void drag();
        }}
        onPointerUp={() => { gesture.current = null; }}
        onPointerCancel={() => { gesture.current = null; dragged.current = true; }}
        onClick={(event) => {
          if (event.detail !== 0 && dragged.current) return;
          poke(); void show(view === "idle" ? "menu" : "idle");
        }}>
        <Live2DPet key={attempt} paused={still} resting={resting} poked={poked} onStatusChange={setPetLoad} />
      </button>
      {petLoad === "error" && <button className="pet-retry" onClick={() => {
        setPetLoad("loading"); setAttempt((old) => old + 1);
      }}>重新加载角色</button>}
      {petLoad === "ready" && (resting || quiet) && <span className="pet-mode" aria-hidden="true">{resting ? "休息中 · Zzz" : "勿扰中"}</span>}
    </section>
  </main>;
}
