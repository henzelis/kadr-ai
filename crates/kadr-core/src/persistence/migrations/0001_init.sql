-- Kadr index, schema v1. Times are Unix milliseconds unless named *_raw.

CREATE TABLE source_root (
    id            INTEGER PRIMARY KEY,
    path          TEXT NOT NULL UNIQUE,
    added_at      INTEGER NOT NULL,
    last_seen_at  INTEGER,
    is_available  INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE media_item (
    id                    INTEGER PRIMARY KEY,
    source_root_id        INTEGER NOT NULL REFERENCES source_root(id) ON DELETE CASCADE,
    path                  TEXT NOT NULL,
    normalized_path       TEXT NOT NULL UNIQUE,
    file_name             TEXT NOT NULL,
    extension             TEXT NOT NULL,
    media_kind            TEXT NOT NULL,
    size_bytes            INTEGER NOT NULL,
    mtime                 INTEGER NOT NULL,
    file_identity         TEXT,
    sha256                TEXT,
    fingerprint_updated_at INTEGER,
    detected_type         TEXT,
    metadata_status       TEXT NOT NULL DEFAULT 'not_read',
    metadata_provider     TEXT,
    thumbnail_path        TEXT,
    captured_at_raw       TEXT,
    captured_at_local     TEXT,
    timezone_offset_min   INTEGER,
    date_source           TEXT,
    date_status           TEXT,
    date_note             TEXT,
    latitude              REAL,
    longitude             REAL,
    altitude              REAL,
    place_label           TEXT,
    location_source       TEXT NOT NULL DEFAULT 'none',
    place_status          TEXT NOT NULL DEFAULT 'no_gps',
    manual_place_override TEXT,
    manual_date_override  TEXT,
    duration_ms           INTEGER,
    width                 INTEGER,
    height                INTEGER,
    video_codec           TEXT,
    audio_codec           TEXT,
    scan_timestamp        INTEGER NOT NULL,
    last_error            TEXT,
    is_sidecar            INTEGER NOT NULL DEFAULT 0,
    sidecar_parent_id     INTEGER REFERENCES media_item(id) ON DELETE SET NULL
);

CREATE INDEX idx_media_captured   ON media_item(captured_at_local);
CREATE INDEX idx_media_place      ON media_item(place_label);
CREATE INDEX idx_media_kind       ON media_item(media_kind);
CREATE INDEX idx_media_sha256     ON media_item(sha256);
CREATE INDEX idx_media_statuses   ON media_item(date_status, place_status, metadata_status);
CREATE INDEX idx_media_root       ON media_item(source_root_id);
CREATE INDEX idx_media_sidecar    ON media_item(sidecar_parent_id);

CREATE TABLE scan_job (
    id           INTEGER PRIMARY KEY,
    started_at   INTEGER NOT NULL,
    finished_at  INTEGER,
    state        TEXT NOT NULL,
    found        INTEGER NOT NULL DEFAULT 0,
    processed    INTEGER NOT NULL DEFAULT 0,
    skipped      INTEGER NOT NULL DEFAULT 0,
    errors       INTEGER NOT NULL DEFAULT 0,
    options_json TEXT NOT NULL DEFAULT '{}'
);

CREATE TABLE operation_plan (
    id               INTEGER PRIMARY KEY,
    created_at       INTEGER NOT NULL,
    destination_root TEXT NOT NULL,
    mode             TEXT NOT NULL DEFAULT 'copy',
    conflict_policy  TEXT NOT NULL DEFAULT 'add_suffix',
    template         TEXT NOT NULL,
    settings_json    TEXT NOT NULL DEFAULT '{}',
    state            TEXT NOT NULL DEFAULT 'draft'
);

CREATE TABLE operation_item (
    id               INTEGER PRIMARY KEY,
    plan_id          INTEGER NOT NULL REFERENCES operation_plan(id) ON DELETE CASCADE,
    media_id         INTEGER REFERENCES media_item(id) ON DELETE SET NULL,
    source_path      TEXT NOT NULL,
    destination_path TEXT NOT NULL,
    expected_size    INTEGER NOT NULL,
    expected_mtime   INTEGER NOT NULL,
    hash_before      TEXT,
    hash_after       TEXT,
    status           TEXT NOT NULL DEFAULT 'planned',
    warnings_json    TEXT NOT NULL DEFAULT '[]',
    error            TEXT,
    started_at       INTEGER,
    finished_at      INTEGER
);

CREATE INDEX idx_op_item_plan   ON operation_item(plan_id, status);
CREATE INDEX idx_op_item_media  ON operation_item(media_id);

CREATE TABLE operation_log (
    id        INTEGER PRIMARY KEY,
    plan_id   INTEGER REFERENCES operation_plan(id) ON DELETE CASCADE,
    item_id   INTEGER REFERENCES operation_item(id) ON DELETE CASCADE,
    at        INTEGER NOT NULL,
    event     TEXT NOT NULL,
    detail    TEXT
);

CREATE INDEX idx_op_log_plan ON operation_log(plan_id, at);

CREATE TABLE app_settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE geocode_cache (
    geobase_version TEXT NOT NULL,
    lat_e4          INTEGER NOT NULL,
    lon_e4          INTEGER NOT NULL,
    city            TEXT,
    region          TEXT,
    country         TEXT,
    PRIMARY KEY (geobase_version, lat_e4, lon_e4)
);
