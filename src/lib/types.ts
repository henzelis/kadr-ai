// DTOs mirroring the serde types in crates/kadr-core. Keep in sync by hand
// until the command surface settles (typed bindings are planned for stage C).

export type MonthFormat = "number_name" | "number" | "name";
export type PlaceGranularity = "city" | "region" | "country";
export type ConflictPolicy = "add_suffix" | "skip" | "ask_user";
export type Theme = "system" | "light" | "dark";
export type OperationMode = "copy" | "move";

export interface AppSettings {
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
  metadata_workers: number;
  thumbnail_workers: number;
  exiftool_path: string | null;
  ffprobe_path: string | null;
  theme: Theme;
}

export type ToolStatus =
  | { tool: "exiftool" | "ffprobe"; program: string; state: "available"; version: string }
  | { tool: "exiftool" | "ffprobe"; program: string; state: "missing" }
  | { tool: "exiftool" | "ffprobe"; program: string; state: "broken"; reason: string };

export interface LibrarySummary {
  media_count: number;
  schema_version: number;
  supported_schema_version: number;
  app_version: string;
}

export interface AppError {
  code: string;
  message: string;
  field: string | null;
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
  note: string | null;
}

export interface GeoPoint {
  latitude: number;
  longitude: number;
  altitude: number | null;
}

/** Library item as the UI shows it. Demo data uses the same shape. */
export interface MediaView {
  id: number;
  path: string;
  file_name: string;
  extension: string;
  kind: MediaKind;
  size_bytes: number;
  sha256: string | null;
  width: number | null;
  height: number | null;
  duration_ms: number | null;
  captured: CapturedDate | null;
  gps: GeoPoint | null;
  place_label: string | null;
  location_source: LocationSource;
  place_status: PlaceStatus;
  sidecars: string[];
  scene: number;
}
