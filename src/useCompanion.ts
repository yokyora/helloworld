import { useCallback, useEffect, useRef, useState } from "react";
import { core, errorMessage, native, preview, type PetState, type SettingsInput, type Snapshot, type Turn } from "./core";

export function useCompanion() {
  const [snapshot, setSnapshot] = useState<Snapshot>(preview);
  const [ready, setReady] = useState(!native);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [streaming, setStreaming] = useState<Turn | null>(null);
  const request = useRef<string | null>(null);
  const actionBusy = useRef(false);
  const mounted = useRef(true);
  const accept = useCallback((next: Snapshot) => {
    if (mounted.current) setSnapshot((current) => next.revision >= current.revision ? next : current);
  }, []);

  const refresh = useCallback(async () => {
    try {
      const next = await core.snapshot();
      accept(next);
      if (mounted.current) setReady(true);
    } catch (e) { if (mounted.current) { setReady(false); setError(errorMessage(e)); } }
  }, [accept]);

  useEffect(() => {
    mounted.current = true;
    if (native) void refresh();
    return () => { mounted.current = false; if (request.current) void core.cancel(request.current).catch(() => {}); };
  }, [refresh]);

  useEffect(() => {
    if (!native || !snapshot.activeRequest || streaming) return;
    const timer = setTimeout(() => void refresh(), 500);
    return () => clearTimeout(timer);
  }, [snapshot, streaming, refresh]);

  async function mutate(action: () => Promise<Snapshot>) {
    if (actionBusy.current || !ready) return false;
    actionBusy.current = true; setBusy(true); setError("");
    try { accept(await action()); return true; }
    catch (e) { setError(errorMessage(e)); return false; }
    finally { actionBusy.current = false; setBusy(false); }
  }
  async function setPet(pet: PetState) {
    if (!native) { setSnapshot((old) => ({ ...old, pet })); return; }
    await mutate(() => core.pet(pet));
  }
  async function saveSettings(input: SettingsInput) { return mutate(() => core.settings(input)); }

  async function send(text: string) {
    if (!native || !ready || request.current || actionBusy.current) return false;
    const id = crypto.randomUUID(); request.current = id;
    setError("");
    setStreaming({ id, userText: text, assistantText: "", status: "pending", createdAt: Math.floor(Date.now() / 1000) });
    let success = false;
    try {
      accept(await core.send(id, text, (delta) => {
        if (mounted.current && request.current === id) setStreaming((old) => old?.id === id ? { ...old, assistantText: old.assistantText + delta } : old);
      }));
      success = true;
    } catch (e) { if (mounted.current) setError(errorMessage(e)); }
    finally {
      if (request.current === id) {
        // Reload persisted status/partial text even when invoke rejected after cancellation.
        await refresh();
        request.current = null;
        if (mounted.current) setStreaming(null);
      }
    }
    return success;
  }
  async function cancel() {
    const id = request.current ?? snapshot.activeRequest;
    if (!id) return;
    try { await core.cancel(id); } catch (e) { setError(errorMessage(e)); }
  }
  const turns = snapshot.turns.filter((turn) => turn.id !== streaming?.id);
  if (streaming) turns.push(streaming);
  return { snapshot, turns, ready, busy, error, setError, refresh, setPet, saveSettings, send, cancel,
    clear: () => mutate(core.clear), sending: streaming !== null || snapshot.activeRequest !== null };
}
