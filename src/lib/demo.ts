// Demonstration records, used only when the UI runs in a plain browser
// (no Rust backend), e.g. for layout checks. They exercise every state the
// UI must show: confirmed EXIF dates, QuickTime dates, file-system fallback,
// unknown dates, GPS without a place name, no GPS, RAW, sidecars, duplicates.

import type { MediaView } from "./types";

export const DEMO_SOURCE = "D:\\DCIM\\Camera Roll";
export const DEMO_DESTINATION = "E:\\Kadr Archive";

const src = (name: string) => `${DEMO_SOURCE}\\${name}`;

export const demoMedia: MediaView[] = [
  {
    id: 1, path: src("2025-08\\IMG_2048.HEIC"), file_name: "IMG_2048.HEIC", extension: "heic", kind: "photo",
    size_bytes: 3_984_588, sha256: "9c4d1f0e7a5b33c8d2e6f19a0b7c4e21d8f3a6b9c0e1d2f3a4b5c6d7e8f9a821",
    width: 4032, height: 3024, duration_ms: null,
    captured: { raw: "2025:08:13 16:42:07", local: "2025-08-13T16:42:07", offset_minutes: 180, source: "exif_original", status: "confirmed", note: null },
    gps: { latitude: 48.4594, longitude: 24.5513, altitude: 528 },
    place_label: "Яремче", location_source: "offline_geocoder", place_status: "resolved",
    mtime: 0, detected_type: null, video_codec: null, audio_codec: null, metadata_status: "read", metadata_provider: "demo", review_reasons: [], warnings: [], last_error: null,
  },
  {
    id: 2, path: src("2025-08\\VID_0912.MOV"), file_name: "VID_0912.MOV", extension: "mov", kind: "video",
    size_bytes: 148_103_022, sha256: "1b2c3d4e5f60718293a4b5c6d7e8f9011a2b3c4d5e6f708192a3b4c5d6e7f801",
    width: 1920, height: 1080, duration_ms: 47_000,
    captured: { raw: "2025:08:14 07:05:31 (UTC)", local: "2025-08-14T10:05:31", offset_minutes: 180, source: "quick_time", status: "confirmed", note: "QuickTime CreateDate записано в UTC; зміщення взято з com.apple.quicktime.creationdate" },
    gps: { latitude: 48.1597, longitude: 24.4999, altitude: 1180 },
    place_label: "Івано-Франківська область", location_source: "offline_geocoder", place_status: "resolved",
    mtime: 0, detected_type: null, video_codec: null, audio_codec: null, metadata_status: "read", metadata_provider: "demo", review_reasons: [], warnings: [], last_error: null,
  },
  {
    id: 3, path: src("2025-12\\IMG_3310.JPG"), file_name: "IMG_3310.JPG", extension: "jpg", kind: "photo",
    size_bytes: 2_716_334, sha256: "aa11bb22cc33dd44ee55ff6600778899aabbccddeeff00112233445566778899",
    width: 4000, height: 3000, duration_ms: null,
    captured: { raw: "2025:12:24 18:10:55", local: "2025-12-24T18:10:55", offset_minutes: 120, source: "exif_original", status: "confirmed", note: null },
    gps: { latitude: 50.4501, longitude: 30.5234, altitude: null },
    place_label: "Київ", location_source: "offline_geocoder", place_status: "resolved",
    mtime: 0, detected_type: null, video_codec: null, audio_codec: null, metadata_status: "read", metadata_provider: "demo", review_reasons: [], warnings: [], last_error: null,
  },
  {
    id: 4, path: src("Paris\\DSC_0142.NEF"), file_name: "DSC_0142.NEF", extension: "nef", kind: "raw_photo",
    size_bytes: 27_115_902, sha256: "5e5e5e5e7a7a7a7a9c9c9c9c1d1d1d1d2f2f2f2f3b3b3b3b4c4c4c4c6d6d6d6d",
    width: 6048, height: 4024, duration_ms: null,
    captured: null,
    gps: { latitude: 48.8584, longitude: 2.2945, altitude: null },
    place_label: "Париж", location_source: "offline_geocoder", place_status: "resolved",
    mtime: 0, detected_type: null, video_codec: null, audio_codec: null, metadata_status: "read", metadata_provider: "demo", review_reasons: [], warnings: [], last_error: null,
  },
  {
    id: 5, path: src("2024-07\\IMG_0711.JPG"), file_name: "IMG_0711.JPG", extension: "jpg", kind: "photo",
    size_bytes: 3_102_440, sha256: "77aa77aa88bb88bb99cc99cc00dd00dd11ee11ee22ff22ff3300330044114411",
    width: 4032, height: 3024, duration_ms: null,
    captured: { raw: "2024:07:19 12:01:44", local: "2024-07-19T12:01:44", offset_minutes: null, source: "exif_original", status: "confirmed", note: "Часовий пояс у файлі відсутній — час не перетворювався" },
    gps: { latitude: 46.4825, longitude: 30.7233, altitude: null },
    place_label: "Одеса", location_source: "offline_geocoder", place_status: "resolved",
    mtime: 0, detected_type: null, video_codec: null, audio_codec: null, metadata_status: "read", metadata_provider: "demo", review_reasons: [], warnings: [], last_error: null,
  },
  {
    id: 6, path: src("2024-07\\IMG_0711 copy.JPG"), file_name: "IMG_0711 copy.JPG", extension: "jpg", kind: "photo",
    size_bytes: 3_102_440, sha256: "77aa77aa88bb88bb99cc99cc00dd00dd11ee11ee22ff22ff3300330044114411",
    width: 4032, height: 3024, duration_ms: null,
    captured: { raw: "2024:07:19 12:01:44", local: "2024-07-19T12:01:44", offset_minutes: null, source: "exif_original", status: "confirmed", note: null },
    gps: { latitude: 46.4825, longitude: 30.7233, altitude: null },
    place_label: "Одеса", location_source: "offline_geocoder", place_status: "resolved",
    mtime: 0, detected_type: null, video_codec: null, audio_codec: null, metadata_status: "read", metadata_provider: "demo", review_reasons: [], warnings: [], last_error: null,
  },
  {
    id: 7, path: src("Сканування\\scan_017.png"), file_name: "scan_017.png", extension: "png", kind: "photo",
    size_bytes: 8_550_117, sha256: null, width: 2480, height: 3508, duration_ms: null,
    captured: { raw: "mtime 2023-03-02 09:14:00", local: "2023-03-02T09:14:00", offset_minutes: null, source: "file_system", status: "file_system_fallback", note: "Дата модифікації файла, не час зйомки" },
    gps: null, place_label: null, location_source: "none", place_status: "no_gps",
    mtime: 0, detected_type: null, video_codec: null, audio_codec: null, metadata_status: "read", metadata_provider: "demo", review_reasons: [], warnings: [], last_error: null,
  },
  {
    id: 8, path: src("2025-05\\PXL_20250503_142233.jpg"), file_name: "PXL_20250503_142233.jpg", extension: "jpg", kind: "photo",
    size_bytes: 4_022_871, sha256: null, width: 4080, height: 3072, duration_ms: null,
    captured: { raw: "2025:05:03 14:22:33", local: "2025-05-03T14:22:33", offset_minutes: 180, source: "exif_original", status: "confirmed", note: null },
    gps: { latitude: 47.0105, longitude: 28.8638, altitude: null },
    place_label: null, location_source: "none", place_status: "gps_no_place",
    mtime: 0, detected_type: null, video_codec: null, audio_codec: null, metadata_status: "read", metadata_provider: "demo", review_reasons: [], warnings: [], last_error: null,
  },
  {
    id: 9, path: src("2025-05\\VID_20250504.mp4"), file_name: "VID_20250504.mp4", extension: "mp4", kind: "video",
    size_bytes: 61_300_442, sha256: null, width: 3840, height: 2160, duration_ms: 12_400,
    captured: { raw: "2025:05:04 00:00:00 / 2025:05:03 21:40:10", local: "2025-05-04T00:00:00", offset_minutes: null, source: "quick_time", status: "needs_review", note: null },
    gps: null, place_label: null, location_source: "none", place_status: "no_gps",
    mtime: 0, detected_type: "MP4", video_codec: "hevc", audio_codec: "aac", metadata_status: "read", metadata_provider: "demo", review_reasons: ["Дати суперечать одна одній: QuickTime:CreateDate = «2025:05:04 00:00:00», Track1:MediaCreateDate = «2025:05:03 21:40:10»"], warnings: [], last_error: null,
  },
];
