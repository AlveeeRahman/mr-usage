// Typed wrappers for the Tauri commands granted in src-tauri/capabilities/main.json.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type Unit = "credits" | "searches" | "percent";
export type Theme = "system" | "light" | "dark";

export interface SnapshotView {
  takenAtMs: number;
  remaining: number | null;
  limit: number | null;
  periodEndMs: number | null;
  extra: [string, string][];
}

export interface Point {
  t: number;
  remaining: number | null;
}

export interface ErrorView {
  kind: string;
  message: string;
}

export interface ProviderView {
  id: string;
  displayName: string;
  docsUrl: string;
  endpoint: string;
  unit: Unit;
  note: string | null;
  enabled: boolean;
  hasKey: boolean;
  pollIntervalSecs: number;
  minPollIntervalSecs: number;
  latest: SnapshotView | null;
  history: Point[];
  lastSuccessMs: number | null;
  nextPollMs: number | null;
  error: ErrorView | null;
}

export interface Settings {
  pollIntervalSecs: number;
  enabled: string[];
  theme: Theme;
}

export interface SettingsView {
  settings: Settings;
  storeLabel: string;
  minPollSecs: number;
  maxPollSecs: number;
}

export const UPDATE_EVENT = "usage:update";

export const listProviders = () => invoke<ProviderView[]>("list_providers");
export const saveKey = (providerId: string, key: string) => invoke<ProviderView>("save_key", { providerId, key });
export const forgetKey = (providerId: string) => invoke<ProviderView>("forget_key", { providerId });
export const keyHint = (providerId: string) => invoke<string | null>("key_hint", { providerId });
export const refresh = (providerId: string) => invoke<ProviderView>("refresh", { providerId });
export const getSettings = () => invoke<SettingsView>("get_settings");
export const updateSettings = (pollIntervalSecs: number, theme: Theme) =>
  invoke<SettingsView>("update_settings", { pollIntervalSecs, theme });
export const wipeNow = () => invoke<void>("wipe_now");

export const onUpdate = (handler: (view: ProviderView) => void): Promise<UnlistenFn> =>
  listen<ProviderView>(UPDATE_EVENT, (event) => handler(event.payload));

/** Tauri rejects with the command's error string; anything else is unexpected. */
export function errorText(err: unknown): string {
  return typeof err === "string" ? err : "Something went wrong.";
}
