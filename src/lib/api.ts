// Typed wrappers around the Rust commands. Outside the Tauri window (plain
// `pnpm dev` in a browser, used for layout checks) an in-memory stand-in with
// demo records answers instead, so the UI renders without a backend.

import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { DEMO_DESTINATION, DEMO_SOURCE, demoMedia } from "./demo";
import { demoDestination, formatShortDate } from "./format";
import { t } from "./i18n";
import type {
  OperationMode, PlanFilter, PlanPage, PlanRow, PlanSummary, RunFailure, RunProgress, RunSummary,
  AppError, AppSettings, Folders, GeobaseInfo, LibrarySummary, MediaPage, MediaQuery, MediaView,
  ScanProgress, ScanReport, ScanStatus, SectionCounts, ToolStatus,
} from "./types";

export const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

const browserDefaults: AppSettings = {
  folder_language: null,
  folder_language_follows_ui: true,
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
  return typeof e === "object" && e !== null && "code" in e;
}

/** User-facing text for an error from the core, in the UI language. */
export function errorMessage(e: unknown): string {
  if (!isAppError(e)) return t.errors.generic;
  return (e.field && t.errorFields[e.field]) || t.errors[e.code] || t.errors.generic;
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

// ---------------------------------------------------------------- thumbnails

/** Address of a cached thumbnail; the mtime makes a changed file reload. */
export function thumbUrl(id: number, size: "grid" | "preview", mtime: number): string {
  return `${convertFileSrc(`${id}.${size}`, "thumb")}?v=${mtime}`;
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

export async function getScanReport(): Promise<ScanReport | null> {
  if (!inTauri) return null;
  return invoke<ScanReport | null>("get_scan_report");
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

/** `?demo=5000` in a plain browser repeats the demo records, for layout and
 *  scrolling checks of large libraries. */
const demoRepeat = typeof location === "undefined" ? 1 : Math.max(1, Number(new URLSearchParams(location.search).get("demo")) || 1);
const demoAll: MediaView[] = Array.from({ length: demoRepeat }, (_, r) =>
  demoMedia.map((m) => (r === 0 ? m : { ...m, id: m.id + r * 1000, file_name: `${r}_${m.file_name}`, sha256: m.sha256 && `${m.sha256}${r}` })),
).flat();

function demoQuery(q: MediaQuery): MediaPage {
  const demoMedia = demoAll;
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
  const withCopies = sorted.slice(q.offset, q.offset + q.limit).map((m) => ({
    ...m,
    duplicates: m.sha256 ? demoMedia.filter((o) => o.sha256 === m.sha256 && o.id !== m.id).map((o) => ({ id: o.id, path: o.path })) : [],
  }));
  return { items: withCopies, total: sorted.length };
}

export async function queryMedia(query: MediaQuery): Promise<MediaPage> {
  if (!inTauri) return demoQuery(query);
  return invoke<MediaPage>("query_media", { query });
}

export async function setManualPlace(ids: number[], place: string | null): Promise<MediaView[]> {
  if (!inTauri) return [];
  return invoke<MediaView[]>("set_manual_place", { ids, place });
}

/** `date` is `YYYY-MM-DDTHH:MM` local time, or null to go back to the metadata. */
export async function setManualDate(ids: number[], date: string | null): Promise<MediaView[]> {
  if (!inTauri) return [];
  return invoke<MediaView[]>("set_manual_date", { ids, date });
}

export async function getSectionCounts(): Promise<SectionCounts> {
  if (!inTauri) {
    const count = (s: MediaQuery["section"]) => demoQuery({ section: s, kind: "all", search: "", sort: "date", offset: 0, limit: 0 }).total;
    return {
      all: demoAll.length,
      review: count("review"),
      nodate: count("no_date"),
      nogps: count("no_gps"),
      duplicates: count("duplicates"),
      with_date: demoAll.filter((m) => m.captured && m.captured.status !== "file_system_fallback").length,
      with_gps: demoAll.filter((m) => m.gps).length,
    };
  }
  return invoke<SectionCounts>("get_section_counts");
}

// ---------------------------------------------------------------- plan & runs

let demoPlan: { summary: PlanSummary; rows: PlanRow[] } | null = null;

function demoBuild(mode: OperationMode): PlanSummary {
  const rows: PlanRow[] = demoAll.slice(0, 200).map((m, i) => {
    const dup = m.sha256 ? demoAll.slice(0, i).some((o) => o.sha256 === m.sha256) : false;
    return {
      id: i + 1, media_id: m.id, source: m.path,
      destination: demoDestination(m, browserSettings, browserSettings.destination_root ?? "E:\\Archive"),
      size: m.size_bytes, captured_local: m.captured?.local ?? null, place: m.place_label,
      status: dup ? "duplicate" : "ready", included: !dup, item_status: dup ? "excluded" : "planned",
      notes: [
        ...(dup ? [{ code: "plan_duplicate", args: { path: demoAll.find((o) => o.sha256 === m.sha256)?.path ?? "" } }] : []),
        ...(m.captured ? [] : [{ code: "plan_no_date" }]),
        ...(m.place_label ? [] : [{ code: "plan_no_place" }]),
      ],
      companions: m.companions.map((c) => ({ id: c.id, file_name: c.file_name, destination: c.file_name, role: c.role })),
    };
  });
  const inc = rows.filter((r) => r.included);
  demoPlan = {
    rows,
    summary: {
      id: 1, created_at: Date.now(), destination_root: browserSettings.destination_root ?? "", mode, state: "draft",
      ready: inc.length, conflicts: 0, already_archived: 0, already_in_place: 0,
      duplicates: rows.length - inc.length, included_files: inc.length + inc.reduce((a, r) => a + r.companions.length, 0),
      included_bytes: inc.reduce((a, r) => a + r.size, 0),
      no_date: rows.filter((r) => !r.captured_local).length, no_place: rows.filter((r) => !r.place).length,
    },
  };
  return demoPlan.summary;
}

export async function buildSortPlan(mode: OperationMode): Promise<PlanSummary> {
  if (!inTauri) return demoBuild(mode);
  return invoke<PlanSummary>("build_sort_plan", { mode });
}

export async function getPlanSummary(planId?: number): Promise<PlanSummary | null> {
  if (!inTauri) return demoPlan?.summary ?? null;
  return invoke<PlanSummary | null>("get_plan_summary", { planId: planId ?? null });
}

export async function getPlanItems(planId: number, filter: PlanFilter, search: string, offset: number, limit: number): Promise<PlanPage> {
  if (!inTauri) {
    const rows = (demoPlan?.rows ?? []).filter((r) =>
      (filter === "all" || (filter === "included" && r.included) || (filter === "excluded" && !r.included)
        || (filter === "attention" && (r.status !== "ready" || !r.captured_local || !r.place)))
      && (!search || `${r.source} ${r.destination}`.toLowerCase().includes(search.toLowerCase())));
    return { items: rows.slice(offset, offset + limit), total: rows.length };
  }
  return invoke<PlanPage>("get_plan_items", { planId, filter, search, offset, limit });
}

export async function setPlanIncluded(planId: number, itemIds: number[], included: boolean): Promise<PlanSummary | null> {
  if (!inTauri) {
    demoPlan?.rows.forEach((r) => { if (itemIds.includes(r.id)) r.included = included; });
    return demoPlan?.summary ?? null;
  }
  return invoke<PlanSummary | null>("set_plan_included", { planId, itemIds, included });
}

export async function setPlanMode(planId: number, mode: OperationMode): Promise<PlanSummary | null> {
  if (!inTauri) {
    if (demoPlan) demoPlan.summary.mode = mode;
    return demoPlan?.summary ?? null;
  }
  return invoke<PlanSummary | null>("set_plan_mode", { planId, mode });
}

export async function applySortPlan(planId: number, mode: OperationMode, files: number): Promise<void> {
  if (!inTauri) throw { code: "generic", field: null } satisfies AppError;
  await invoke("apply_sort_plan", { planId, confirmation: { mode, files } });
}

export async function undoSortPlan(planId: number): Promise<void> {
  if (!inTauri) return;
  await invoke("undo_sort_plan", { planId });
}

export async function cancelRun(): Promise<void> {
  if (!inTauri) return;
  await invoke("cancel_run");
}

export async function getRunLog(): Promise<{ running: boolean; runs: RunSummary[] }> {
  if (!inTauri) return { running: false, runs: [] };
  return invoke("get_run_log");
}

export async function getRunFailures(planId: number): Promise<RunFailure[]> {
  if (!inTauri) return [];
  return invoke<RunFailure[]>("get_run_failures", { planId });
}

export async function onRunEvents(handlers: {
  progress: (p: RunProgress) => void;
  finished: (kind: "apply" | "undo", p: RunProgress) => void;
  failed: (e: AppError) => void;
}): Promise<UnlistenFn> {
  if (!inTauri) return () => {};
  const offs = await Promise.all([
    listen<RunProgress>("run-progress", (e) => handlers.progress(e.payload)),
    listen<{ kind: "apply" | "undo"; progress: RunProgress }>("run-finished", (e) => handlers.finished(e.payload.kind, e.payload.progress)),
    listen<AppError>("run-failed", (e) => handlers.failed(e.payload)),
  ]);
  return () => offs.forEach((off) => off());
}
