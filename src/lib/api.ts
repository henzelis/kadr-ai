// Typed wrappers around the Rust commands. Outside the Tauri window (plain
// `pnpm dev` in a browser, used for UI checks) an in-memory stand-in answers
// instead, so the shell renders without a backend.

import { invoke } from "@tauri-apps/api/core";
import type { AppError, AppSettings, LibrarySummary, ToolStatus } from "./types";

export const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

const browserDefaults: AppSettings = {
  folder_template: "{year}/{month}/{place}/{filename}",
  month_format: "number_name",
  unknown_date_label: "Невідома дата",
  unknown_place_label: "Без геолокації",
  place_granularity: "city",
  coordinates_as_place: false,
  filesystem_date_fallback: false,
  offline_geocoding: true,
  network_enabled: false,
  conflict_policy: "add_suffix",
  follow_symlinks: false,
  skip_system_dirs: true,
  metadata_workers: 4,
  thumbnail_workers: 2,
  exiftool_path: null,
  ffprobe_path: null,
  theme: "system",
};
let browserSettings: AppSettings = { ...browserDefaults };

export function isAppError(e: unknown): e is AppError {
  return typeof e === "object" && e !== null && "code" in e && "message" in e;
}

export function errorMessage(e: unknown): string {
  return isAppError(e) ? e.message : "Не вдалося виконати дію.";
}

export async function getSettings(): Promise<AppSettings> {
  if (!inTauri) return { ...browserSettings };
  return invoke<AppSettings>("get_settings");
}

export async function updateSettings(settings: AppSettings): Promise<AppSettings> {
  if (!inTauri) {
    browserSettings = { ...settings };
    return { ...browserSettings };
  }
  return invoke<AppSettings>("update_settings", { settings });
}

export async function getToolStatus(): Promise<ToolStatus[]> {
  if (!inTauri) {
    return [
      { tool: "exiftool", program: "exiftool", state: "missing" },
      { tool: "ffprobe", program: "ffprobe", state: "missing" },
    ];
  }
  return invoke<ToolStatus[]>("get_tool_status");
}

export async function getLibrarySummary(): Promise<LibrarySummary | null> {
  if (!inTauri) return null;
  return invoke<LibrarySummary>("get_library_summary");
}
