import { Channel, invoke, isTauri } from "@tauri-apps/api/core";

export interface PetState { resting: boolean; quiet: boolean }
export interface DisplayScale { petPercent: number; panelPercent: number }
export interface BubbleAnchor { x: number; y: number }
export interface ModelLayout { modelId: string; x: number; y: number; width: number; height: number; defaultAnchor: BubbleAnchor | null }
export interface BubbleCalibration { modelReady: boolean; calibrated: boolean; active: boolean }
export type ShellView = "idle" | "menu" | "chat" | "settings" | "privacy";
export interface PetStatus { revision: number; pet: PetState; sending: boolean }
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
  registerBubbleModel: (layout: ModelLayout) => invoke<void>("register_bubble_model", { layout }),
  bubbleCalibration: () => invoke<BubbleCalibration>("get_bubble_calibration"),
  startBubbleCalibration: () => invoke<void>("start_bubble_calibration"),
  finishBubbleCalibration: (point: BubbleAnchor | null) => invoke<void>("finish_bubble_calibration", { point }),
  displayScale: () => invoke<DisplayScale>("get_display_scale"),
  setDisplayScale: (scales: DisplayScale) => invoke<DisplayScale>("set_display_scale", { scales }),
  shell: (mode: ShellView) => invoke<void>("set_shell_mode", { mode }),
  panelReady: () => invoke<void>("panel_ready"),
  shellAction: (action: "minimize" | "exit") => invoke<void>("shell_action", { action }),
  petStatus: () => invoke<PetStatus>("get_pet_status"),
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
