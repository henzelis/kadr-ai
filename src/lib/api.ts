// Typed wrappers around the Rust commands. Outside the Tauri window (plain
// `pnpm dev` in a browser, used for layout checks) an in-memory stand-in with
// demo records answers instead, so the UI renders without a backend.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { DEMO_DESTINATION, DEMO_SOURCE, demoMedia } from "./demo";
import { formatShortDate } from "./format";
import type {
  AppError, AppSettings, Folders, GeobaseInfo, LibrarySummary, MediaPage, MediaQuery, MediaView,
  ScanProgress, ScanStatus, SectionCounts, ToolStatus,
} from "./types";

export const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

const browserDefaults: AppSettings = {
  destination_root: DEMO_DESTINATION,
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
  compute_hashes: true,
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

// ---------------------------------------------------------------- settings

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
      { tool: "exiftool", program: "exiftool", state: "available", version: "13.59", source: "bundled" },
      { tool: "ffprobe", program: "ffprobe", state: "available", version: "8.1.3", source: "bundled" },
      { tool: "ffmpeg", program: "ffmpeg", state: "missing" },
    ];
  }
  return invoke<ToolStatus[]>("get_tool_status");
}

export async function getLibrarySummary(): Promise<LibrarySummary | null> {
  if (!inTauri) return null;
  return invoke<LibrarySummary>("get_library_summary");
}

// ---------------------------------------------------------------- geobase

export async function getGeobaseStatus(): Promise<GeobaseInfo | null> {
  if (!inTauri) return null;
  return invoke<GeobaseInfo | null>("get_geobase_status");
}

/** Lets the user pick a geobase file and installs it; `null` if cancelled. */
export async function pickGeobaseFile(): Promise<GeobaseInfo | null> {
  if (!inTauri) return null;
  const picked = await open({
    multiple: false,
    directory: false,
    title: "Файл геобази Kadr",
    filters: [{ name: "Геобаза Kadr", extensions: ["sqlite3"] }],
  });
  if (!picked || Array.isArray(picked)) return null;
  return invoke<GeobaseInfo>("import_geobase", { path: picked });
}

// ---------------------------------------------------------------- folders

export async function getFolders(): Promise<Folders> {
  if (!inTauri) {
    return {
      sources: [{ id: 1, path: DEMO_SOURCE, added_at: 0, is_available: true }],
      destination: browserSettings.destination_root,
      destination_inside_source: false,
    };
  }
  return invoke<Folders>("get_folders");
}

/** Opens the system folder picker; `null` when the user cancelled. */
export async function pickSourceFolders(): Promise<Folders | null> {
  if (!inTauri) return null;
  const picked = await open({ directory: true, multiple: true, title: "Папки з фото й відео" });
  if (!picked || picked.length === 0) return null;
  return invoke<Folders>("add_source_roots", { paths: Array.isArray(picked) ? picked : [picked] });
}

export async function pickDestinationFolder(): Promise<Folders | null> {
  if (!inTauri) return null;
  const picked = await open({ directory: true, multiple: false, title: "Папка для впорядкованого архіву" });
  if (!picked || Array.isArray(picked)) return null;
  return invoke<Folders>("set_destination_root", { path: picked });
}

export async function removeSourceRoot(id: number): Promise<Folders> {
  return invoke<Folders>("remove_source_root", { id });
}

// ---------------------------------------------------------------- scanning

export async function startScan(): Promise<void> {
  if (!inTauri) return;
  await invoke("start_scan");
}

export async function cancelScan(): Promise<void> {
  if (!inTauri) return;
  await invoke("cancel_scan");
}

export async function getScanStatus(): Promise<ScanStatus> {
  if (!inTauri) return { running: false, progress: null, last: null };
  return invoke<ScanStatus>("get_scan_status");
}

export async function onScanEvents(handlers: {
  progress: (p: ScanProgress) => void;
  finished: (p: ScanProgress) => void;
  failed: (e: AppError) => void;
}): Promise<UnlistenFn> {
  if (!inTauri) return () => {};
  const offs = await Promise.all([
    listen<ScanProgress>("scan-progress", (e) => handlers.progress(e.payload)),
    listen<ScanProgress>("scan-finished", (e) => handlers.finished(e.payload)),
    listen<AppError>("scan-failed", (e) => handlers.failed(e.payload)),
  ]);
  return () => offs.forEach((off) => off());
}

// ---------------------------------------------------------------- library

const duplicateIds = (items: MediaView[]) => {
  const byHash = new Map<string, number>();
  for (const m of items) if (m.sha256) byHash.set(m.sha256, (byHash.get(m.sha256) ?? 0) + 1);
  return new Set(items.filter((m) => m.sha256 && (byHash.get(m.sha256) ?? 0) > 1).map((m) => m.id));
};

const demoIn = {
  all: () => true,
  review: (m: MediaView) =>
    m.review_reasons.length > 0 || ["failed", "unavailable", "partial"].includes(m.metadata_status)
    || m.captured?.status === "needs_review" || m.captured?.status === "file_system_fallback",
  no_date: (m: MediaView) => m.captured === null,
  no_gps: (m: MediaView) => m.place_status !== "resolved",
};

function demoQuery(q: MediaQuery): MediaPage {
  const dups = duplicateIds(demoMedia);
  const inSection = q.section === "duplicates" ? (m: MediaView) => dups.has(m.id) : demoIn[q.section];
  const needle = q.search.trim().toLocaleLowerCase("uk");
  const list = demoMedia.filter((m) => {
    if (!inSection(m)) return false;
    if (q.kind === "photo" && m.kind !== "photo" && m.kind !== "raw_photo") return false;
    if (q.kind === "video" && m.kind !== "video") return false;
    if (!needle) return true;
    const date = m.captured ? formatShortDate(m.captured.local) : "";
    return [m.file_name, m.path, m.place_label ?? "", date].some((s) => s.toLocaleLowerCase("uk").includes(needle));
  });
  const byDate = (m: MediaView) => m.captured?.local ?? "";
  const sorted = list.toSorted((a, b) =>
    q.sort === "name" ? a.file_name.localeCompare(b.file_name, "uk")
    : q.sort === "size" ? b.size_bytes - a.size_bytes
    : byDate(b).localeCompare(byDate(a)),
  );
  return { items: sorted.slice(q.offset, q.offset + q.limit), total: sorted.length };
}

export async function queryMedia(query: MediaQuery): Promise<MediaPage> {
  if (!inTauri) return demoQuery(query);
  return invoke<MediaPage>("query_media", { query });
}

export async function getSectionCounts(): Promise<SectionCounts> {
  if (!inTauri) {
    const count = (s: MediaQuery["section"]) => demoQuery({ section: s, kind: "all", search: "", sort: "date", offset: 0, limit: 0 }).total;
    return {
      all: demoMedia.length,
      review: count("review"),
      nodate: count("no_date"),
      nogps: count("no_gps"),
      duplicates: count("duplicates"),
      with_date: demoMedia.filter((m) => m.captured && m.captured.status !== "file_system_fallback").length,
      with_gps: demoMedia.filter((m) => m.gps).length,
    };
  }
  return invoke<SectionCounts>("get_section_counts");
}
