// DTOs mirroring the serde types in crates/kadr-core. Keep in sync by hand
// until the command surface settles (typed bindings are planned for stage C).

export type MonthFormat = "number_name" | "number" | "name";
export type PlaceGranularity = "city" | "region" | "country";
export type ConflictPolicy = "add_suffix" | "skip" | "ask_user";
export type Theme = "system" | "light" | "dark";
export type OperationMode = "copy" | "move";

export interface AppSettings {
  folder_language: "uk" | "en" | null;
  destination_root: string | null;
  folder_template: string;
  month_format: MonthFormat;
  unknown_date_label: string;
  unknown_place_label: string;
  place_granularity: PlaceGranularity;
  coordinates_as_place: boolean;
  filesystem_date_fallback: boolean;
  offline_geocoding: boolean;
  network_enabled: boolean;
  conflict_policy: ConflictPolicy;
  follow_symlinks: boolean;
  skip_system_dirs: boolean;
  compute_hashes: boolean;
  metadata_workers: number;
  thumbnail_workers: number;
  exiftool_path: string | null;
  ffprobe_path: string | null;
  theme: Theme;
}

export type ToolName = "exiftool" | "ffprobe" | "ffmpeg";
export type ToolSource = "custom" | "bundled" | "system";
export type ToolStatus =
  | { tool: ToolName; program: string; state: "available"; version: string; source: ToolSource }
  | { tool: ToolName; program: string; state: "missing" }
  | { tool: ToolName; program: string; state: "broken"; reason: string };

export interface LibrarySummary {
  media_count: number;
  schema_version: number;
  supported_schema_version: number;
  app_version: string;
}

export interface AppError {
  code: string;
  field: string | null;
}

/** Message from the core: a code plus parameters, translated by the UI. */
export interface Note {
  code: string;
  args?: Record<string, string>;
}

export type MediaKind = "photo" | "raw_photo" | "video" | "sidecar" | "unsupported";
export type DateSource =
  | "exif_original" | "exif_other" | "xmp" | "iptc" | "quick_time"
  | "helper_other" | "file_system" | "manual";
export type DateStatus = "confirmed" | "file_system_fallback" | "needs_review" | "manual";
export type LocationSource = "manual" | "metadata_text" | "offline_geocoder" | "none";
export type PlaceStatus = "resolved" | "gps_no_place" | "no_gps";

export interface CapturedDate {
  raw: string;
  local: string; // NaiveDateTime, "YYYY-MM-DDTHH:MM:SS"
  offset_minutes: number | null;
  source: DateSource;
  status: DateStatus;
  note: Note | null;
}

export interface GeoPoint {
  latitude: number;
  longitude: number;
  altitude: number | null;
}

export type MetadataStatus = "not_read" | "read" | "partial" | "unavailable" | "failed";

export interface Companion {
  id: number;
  file_name: string;
  role: "sidecar" | "live_video";
  size_bytes: number;
}

/** Library item as returned by `query_media` (Rust `MediaRow`). */
export interface MediaView {
  companions: Companion[];
  duplicates: { id: number; path: string }[];
  id: number;
  path: string;
  mtime: number;
  file_name: string;
  extension: string;
  kind: MediaKind;
  size_bytes: number;
  sha256: string | null;
  detected_type: string | null;
  width: number | null;
  height: number | null;
  duration_ms: number | null;
  video_codec: string | null;
  audio_codec: string | null;
  captured: CapturedDate | null;
  gps: GeoPoint | null;
  place_label: string | null;
  location_source: LocationSource;
  place_status: PlaceStatus;
  metadata_status: MetadataStatus;
  metadata_readers: string[];
  review_reasons: Note[];
  warnings: Note[];
  last_error: Note | null;
}

export type LibrarySection = "all" | "review" | "no_date" | "no_gps" | "duplicates";
export type KindFilter = "all" | "photo" | "video";
export type SortKey = "date" | "name" | "size";

export interface MediaQuery {
  section: LibrarySection;
  kind: KindFilter;
  search: string;
  sort: SortKey;
  offset: number;
  limit: number;
}

export interface MediaPage {
  items: MediaView[];
  total: number;
}

export interface SectionCounts {
  all: number;
  review: number;
  nodate: number;
  nogps: number;
  duplicates: number;
  with_date: number;
  with_gps: number;
}

export interface SourceRoot {
  id: number;
  path: string;
  added_at: number;
  is_available: boolean;
}

export interface Folders {
  sources: SourceRoot[];
  destination: string | null;
  destination_inside_source: boolean;
}

export type ScanPhase = "walking" | "reading" | "done" | "cancelled";

export interface ScanProgress {
  job_id: number;
  phase: ScanPhase;
  root: string | null;
  found: number;
  processed: number;
  unchanged: number;
  skipped: number;
  errors: number;
  pending: number;
  current: string | null;
}

export interface LastScan {
  job_id: number;
  finished_at: number;
  state: "done" | "cancelled" | "interrupted";
  found: number;
  processed: number;
  unchanged: number;
  skipped: number;
  errors: number;
}

export interface ScanStatus {
  running: boolean;
  progress: ScanProgress | null;
  last: LastScan | null;
}

export interface GeobaseInfo {
  version: string;
  source: string;
  places: number;
  built_at: string;
}

export interface ScanReport {
  summary: LastScan;
  in_library: number;
  companions: number;
  missing: number;
  not_media_by_extension: { extension: string; count: number }[];
  issues: { path: string; kind: string; message: string | null }[];
}

export type PlanStatus = "ready" | "conflict" | "already_archived" | "already_in_place" | "duplicate";
export type PlanFilter = "all" | "included" | "excluded" | "attention";
export type ItemStatus = "planned" | "excluded" | "in_progress" | "done" | "failed" | "skipped" | "undone" | "interrupted";

export interface PlanSummary {
  id: number;
  created_at: number;
  destination_root: string;
  mode: OperationMode;
  state: "draft" | "running" | "partial" | "done" | "undoing" | "undone";
  ready: number;
  conflicts: number;
  already_archived: number;
  already_in_place: number;
  duplicates: number;
  included_files: number;
  included_bytes: number;
  no_date: number;
  no_place: number;
}

export interface PlanRow {
  id: number;
  media_id: number | null;
  source: string;
  destination: string;
  size: number;
  captured_local: string | null;
  place: string | null;
  status: PlanStatus;
  included: boolean;
  item_status: ItemStatus;
  notes: Note[];
  companions: { id: number; file_name: string; destination: string; role: "main" | "sidecar" | "live_video" }[];
}

export interface PlanPage {
  items: PlanRow[];
  total: number;
}

export interface RunProgress {
  plan_id: number;
  mode: OperationMode;
  phase: "running" | "undoing" | "done" | "partial";
  total: number;
  done: number;
  failed: number;
  skipped: number;
  bytes_total: number;
  bytes_done: number;
  current: string | null;
}

export interface RunSummary {
  plan_id: number;
  created_at: number;
  destination_root: string;
  mode: OperationMode;
  state: string;
  done: number;
  failed: number;
  undone: number;
  pending: number;
  bytes_done: number;
}

export interface RunFailure {
  source: string;
  destination: string;
  reason: Note | null;
}
