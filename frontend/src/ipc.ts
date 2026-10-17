// Typed wrappers for the Tauri commands granted in src-tauri/capabilities/main.json.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

// Money units are whole minor units: cents for "usd" and "eur", fen for "cny".
export type Unit = "credits" | "searches" | "requests" | "percent" | "usd" | "cny" | "eur" | "characters" | "seconds";
export type Category = "search" | "data" | "llm" | "media" | "compute" | "quantum" | "seo";
export type Theme = "system" | "light" | "dark";
export type View = "pinned" | "all";

export interface ConfigField {
  label: string;
  hint: string;
  placeholder: string;
}

export interface SnapshotView {
  takenAtMs: number;
  unit: Unit;
  remaining: number | null;
  limit: number | null;
  periodEndMs: number | null;
  spent: number | null;
  spendPerHour: number | null;
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
  category: Category;
  note: string | null;
  configField: ConfigField | null;
  config: string | null;
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
  pinned: string[];
  collapsed: string[];
  view: View;
  providerConfig: Record<string, string>;
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
export const updateLayout = (view: View, pinned: string[], collapsed: string[]) =>
  invoke<SettingsView>("update_layout", { view, pinned, collapsed });
export const setProviderConfig = (providerId: string, value: string | null) =>
  invoke<ProviderView>("set_provider_config", { providerId, value });
export const wipeNow = () => invoke<void>("wipe_now");

// The returned UnlistenFn must not be called: `core:event:allow-unlisten` is
// deliberately not granted, because the app listens for its whole lifetime.
export const onUpdate = (handler: (view: ProviderView) => void): Promise<UnlistenFn> =>
  listen<ProviderView>(UPDATE_EVENT, (event) => handler(event.payload));

/** Tauri rejects with the command's error string; anything else is unexpected. */
export function errorText(err: unknown): string {
  return typeof err === "string" ? err : "Something went wrong.";
}
