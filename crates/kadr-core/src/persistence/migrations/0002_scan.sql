-- Stage B: indexing details.

-- Requested metadata tags as read (small JSON object), so dates and places can
-- be recomputed after a settings change without touching the files again.
ALTER TABLE media_item ADD COLUMN metadata_json TEXT;
-- Why the item needs a human look (JSON array of short Ukrainian sentences).
ALTER TABLE media_item ADD COLUMN review_reasons TEXT NOT NULL DEFAULT '[]';
-- Non-blocking notes, e.g. extension does not match content.
ALTER TABLE media_item ADD COLUMN warnings TEXT NOT NULL DEFAULT '[]';
-- Explicit location text found in metadata (IPTC/XMP).
ALTER TABLE media_item ADD COLUMN place_city TEXT;
ALTER TABLE media_item ADD COLUMN place_region TEXT;
ALTER TABLE media_item ADD COLUMN place_country TEXT;
-- The file was not found by the last complete scan of its root.
ALTER TABLE media_item ADD COLUMN is_missing INTEGER NOT NULL DEFAULT 0;

-- Lower-cased name, path, place and date for search (SQLite's LOWER() only
-- folds ASCII, so Cyrillic is folded in Rust when the row is written).
ALTER TABLE media_item ADD COLUMN search_text TEXT NOT NULL DEFAULT '';

CREATE INDEX idx_media_missing ON media_item(is_missing);

ALTER TABLE scan_job ADD COLUMN unchanged INTEGER NOT NULL DEFAULT 0;
ALTER TABLE scan_job ADD COLUMN error_detail TEXT;

-- Files skipped or failed during a scan, for the scan report.
CREATE TABLE scan_issue (
    id       INTEGER PRIMARY KEY,
    job_id   INTEGER NOT NULL REFERENCES scan_job(id) ON DELETE CASCADE,
    path     TEXT NOT NULL,
    kind     TEXT NOT NULL,
    message  TEXT
);

CREATE INDEX idx_scan_issue_job ON scan_issue(job_id);
