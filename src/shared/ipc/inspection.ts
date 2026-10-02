import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";

export async function canInspectCurrentWindow(): Promise<boolean> {
  if (getCurrentWebviewWindow().label !== "overview") return false;
  try {
    await invoke<void>("plugin:mcp|push_ipc");
    return true;
  } catch {
    return false;
  }
}
