import { Channel, invoke, isTauri } from "@tauri-apps/api/core";

export interface PetState { resting: boolean; quiet: boolean }
export interface Settings { model: string; hasApiKey: boolean; allowRemote: boolean }
export interface Turn {
  id: string;
  userText: string;
  assistantText: string;
  status: "pending" | "complete" | "cancelled" | "error";
  createdAt: number;
}
export interface Snapshot {
  revision: number;
  pet: PetState;
  settings: Settings;
  turns: Turn[];
  activeRequest: string | null;
}
export interface SettingsInput { model: string; apiKey?: string; removeKey: boolean; allowRemote: boolean }
export const native = isTauri();
export const preview: Snapshot = {
  revision: 0, pet: { resting: false, quiet: false },
  settings: { model: "deepseek-flash", hasApiKey: false, allowRemote: false },
  turns: [], activeRequest: null,
};
export function errorMessage(error: unknown): string {
  if (typeof error === "object" && error !== null && "message" in error && typeof error.message === "string") return error.message;
  return "操作未完成，请重试或重新启动桌宠。";
}
export const core = {
  snapshot: () => invoke<Snapshot>("get_snapshot"),
  pet: (pet: PetState) => invoke<Snapshot>("set_pet_state", { ...pet }),
  settings: (input: SettingsInput) => invoke<Snapshot>("save_settings", { input }),
  clear: () => invoke<Snapshot>("clear_conversation"),
  cancel: (requestId: string) => invoke<void>("cancel_reply", { requestId }),
  send(requestId: string, text: string, onText: (text: string) => void) {
    const onDelta = new Channel<{ requestId: string; text: string }>();
    onDelta.onmessage = (event) => { if (event.requestId === requestId) onText(event.text); };
    return invoke<Snapshot>("send_message", { requestId, text, onDelta });
  },
};
