import { t } from "./i18n/uk";
import type { AppSettings, CapturedDate, MediaView, MonthFormat } from "./types";

const MONTHS_UK = [
  "Січень", "Лютий", "Березень", "Квітень", "Травень", "Червень",
  "Липень", "Серпень", "Вересень", "Жовтень", "Листопад", "Грудень",
];
const MONTHS_GENITIVE_UK = [
  "січня", "лютого", "березня", "квітня", "травня", "червня",
  "липня", "серпня", "вересня", "жовтня", "листопада", "грудня",
];

const number = new Intl.NumberFormat("uk-UA");

export function formatCount(n: number): string {
  return number.format(n);
}

export function formatSize(bytes: number): string {
  const units = ["Б", "КБ", "МБ", "ГБ", "ТБ"];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  const digits = unit === 0 || value >= 100 ? 0 : 1;
  return `${value.toLocaleString("uk-UA", { maximumFractionDigits: digits, minimumFractionDigits: digits })} ${units[unit]}`;
}

/** Parses a NaiveDateTime string without applying any time zone. */
function parts(local: string): { y: number; m: number; d: number; hh: string; mm: string } | null {
  const match = /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2})/.exec(local);
  if (!match) return null;
  return { y: Number(match[1]), m: Number(match[2]), d: Number(match[3]), hh: match[4], mm: match[5] };
}

export function formatCaptured(local: string, offsetMinutes: number | null): string {
  const p = parts(local);
  if (!p) return local;
  const base = `${p.d} ${MONTHS_GENITIVE_UK[p.m - 1]} ${p.y}, ${p.hh}:${p.mm}`;
  if (offsetMinutes === null) return base;
  const sign = offsetMinutes < 0 ? "−" : "+";
  const abs = Math.abs(offsetMinutes);
  return `${base} (UTC${sign}${String(Math.floor(abs / 60)).padStart(2, "0")}:${String(abs % 60).padStart(2, "0")})`;
}

export function formatShortDate(local: string): string {
  const p = parts(local);
  return p ? `${String(p.d).padStart(2, "0")}.${String(p.m).padStart(2, "0")}.${p.y}` : local;
}

export function formatDuration(ms: number): string {
  const total = Math.round(ms / 1000);
  return `${Math.floor(total / 60)}:${String(total % 60).padStart(2, "0")}`;
}

export function formatCoords(lat: number, lon: number): string {
  const ns = lat >= 0 ? "N" : "S";
  const ew = lon >= 0 ? "E" : "W";
  return `${Math.abs(lat).toFixed(4)}° ${ns}, ${Math.abs(lon).toFixed(4)}° ${ew}`;
}

export function monthFolder(format: MonthFormat, month: number): string {
  const mm = String(month).padStart(2, "0");
  const name = MONTHS_UK[month - 1] ?? "";
  if (format === "number") return mm;
  if (format === "name") return name;
  return `${mm} - ${name}`;
}

/**
 * Destination preview for demo records only. The real planner (template
 * parser, sanitizer, conflicts) is Rust code arriving in stage D; this
 * function exists so the stage-A shell can react to settings.
 */
export function demoDestination(item: MediaView, settings: AppSettings, root: string): string {
  const sep = root.includes("\\") ? "\\" : "/";
  const captured = effectiveDate(item, settings);
  const p = captured ? parts(captured.local) : null;
  const place = item.place_label ?? settings.unknown_place_label;
  const segments = p
    ? [String(p.y), monthFolder(settings.month_format, p.m), place]
    : [settings.unknown_date_label, place];
  return [root, ...segments, item.file_name].join(sep);
}

/**
 * The capture date the planner may use. A file-system date counts only while
 * the user has that fallback switched on; otherwise the date is unknown.
 */
export function effectiveDate(item: MediaView, settings: AppSettings): CapturedDate | null {
  if (item.captured?.status === "file_system_fallback" && !settings.filesystem_date_fallback) return null;
  return item.captured;
}

export type MetaState = { label: string; tone: "ok" | "review" | "missing" };

/** One badge per card: the most important problem, or "ok". */
export function metaState(item: MediaView): MetaState {
  if (item.metadata_status === "failed" || item.metadata_status === "unavailable" || item.metadata_status === "partial") {
    return { label: t.metadataStatus[item.metadata_status], tone: "review" };
  }
  if (item.review_reasons.length > 0) return { label: t.state.review, tone: "review" };
  if (!item.captured) return { label: t.state.noDate, tone: "missing" };
  if (item.captured.status === "needs_review") return { label: t.state.review, tone: "review" };
  if (item.captured.status === "file_system_fallback") return { label: t.state.fsDate, tone: "review" };
  if (item.place_status === "gps_no_place") return { label: t.state.gpsNoPlace, tone: "review" };
  if (item.place_status === "no_gps") return { label: t.state.noGps, tone: "missing" };
  return { label: t.state.ok, tone: "ok" };
}
