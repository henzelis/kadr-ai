use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};

use super::migrations::MIGRATIONS;
use crate::errors::{CoreError, CoreResult};
use crate::settings::AppSettings;

const SETTINGS_KEY: &str = "settings";

/// The local index. Opening it applies pending migrations.
pub struct Db {
    conn: Connection,
}

impl Db {
    pub fn open(path: &Path) -> CoreResult<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        Self::init(Connection::open(path)?)
    }

    pub fn open_in_memory() -> CoreResult<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> CoreResult<Self> {
        conn.pragma_update(None, "foreign_keys", "ON")?;
        // WAL keeps the UI's reads from blocking the scanner's writes.
        // In-memory databases report "memory" here; that is fine.
        let _mode: String =
            conn.pragma_update_and_check(None, "journal_mode", "WAL", |r| r.get(0))?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        let mut db = Self { conn };
        db.migrate()?;
        Ok(db)
    }

    /// Current schema version.
    pub fn schema_version(&self) -> CoreResult<i64> {
        Ok(self
            .conn
            .pragma_query_value(None, "user_version", |r| r.get(0))?)
    }

    /// Latest schema version this build knows.
    pub fn supported_version() -> i64 {
        i64::try_from(MIGRATIONS.len()).unwrap_or(i64::MAX)
    }

    fn migrate(&mut self) -> CoreResult<()> {
        let current = self.schema_version()?;
        let supported = Self::supported_version();
        if current > supported {
            return Err(CoreError::SchemaTooNew {
                found: current,
                supported,
            });
        }
        let pending = usize::try_from(current).unwrap_or(0);
        for (version, sql) in (1_i64..).zip(MIGRATIONS.iter()).skip(pending) {
            let tx = self.conn.transaction()?;
            tx.execute_batch(sql)?;
            tx.pragma_update(None, "user_version", version)?;
            tx.commit()?;
        }
        Ok(())
    }

    /// Stored settings, or defaults when none were saved yet.
    pub fn load_settings(&self) -> CoreResult<AppSettings> {
        let json: Option<String> = self
            .conn
            .query_row(
                "SELECT value FROM app_settings WHERE key = ?1",
                params![SETTINGS_KEY],
                |r| r.get(0),
            )
            .optional()?;
        match json {
            Some(json) => Ok(serde_json::from_str(&json)?),
            None => Ok(AppSettings::default()),
        }
    }

    /// Validates and stores settings.
    pub fn save_settings(&self, settings: &AppSettings) -> CoreResult<()> {
        settings.validate()?;
        let json = serde_json::to_string(settings)?;
        self.conn.execute(
            "INSERT INTO app_settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![SETTINGS_KEY, json],
        )?;
        Ok(())
    }

    /// Number of indexed media items (sidecars excluded).
    pub fn media_count(&self) -> CoreResult<u64> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM media_item WHERE is_sidecar = 0",
            [],
            |r| r.get(0),
        )?;
        Ok(u64::try_from(count).unwrap_or(0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Theme;

    fn table_names(db: &Db) -> Vec<String> {
        let mut stmt = db
            .conn
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
            .unwrap();
        stmt.query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    }

    #[test]
    fn fresh_database_gets_full_schema() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(db.schema_version().unwrap(), Db::supported_version());
        let tables = table_names(&db);
        for t in [
            "app_settings",
            "geocode_cache",
            "media_item",
            "operation_item",
            "operation_log",
            "operation_plan",
            "scan_job",
            "source_root",
        ] {
            assert!(tables.iter().any(|n| n == t), "missing table {t}");
        }
        assert_eq!(db.media_count().unwrap(), 0);
    }

    #[test]
    fn reopening_keeps_data_and_does_not_rerun_migrations() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("kadr.sqlite3");
        {
            let db = Db::open(&path).unwrap();
            db.conn
                .execute(
                    "INSERT INTO source_root (path, added_at) VALUES ('D:/Photos', 1)",
                    [],
                )
                .unwrap();
        }
        let db = Db::open(&path).unwrap();
        let count: i64 = db
            .conn
            .query_row("SELECT COUNT(*) FROM source_root", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
        assert_eq!(db.schema_version().unwrap(), Db::supported_version());
    }

    #[test]
    fn refuses_database_from_newer_build() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("kadr.sqlite3");
        drop(Db::open(&path).unwrap());
        let conn = Connection::open(&path).unwrap();
        conn.pragma_update(None, "user_version", Db::supported_version() + 1)
            .unwrap();
        drop(conn);
        assert!(matches!(
            Db::open(&path),
            Err(CoreError::SchemaTooNew { .. })
        ));
    }

    #[test]
    fn settings_round_trip_and_default_when_absent() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(db.load_settings().unwrap(), AppSettings::default());
        let changed = AppSettings {
            theme: Theme::Dark,
            filesystem_date_fallback: true,
            ..AppSettings::default()
        };
        db.save_settings(&changed).unwrap();
        db.save_settings(&changed).unwrap();
        assert_eq!(db.load_settings().unwrap(), changed);
    }

    #[test]
    fn invalid_settings_are_not_stored() {
        let db = Db::open_in_memory().unwrap();
        let bad = AppSettings {
            unknown_date_label: "../x".to_owned(),
            ..AppSettings::default()
        };
        assert!(db.save_settings(&bad).is_err());
        assert_eq!(db.load_settings().unwrap(), AppSettings::default());
    }

    #[test]
    fn duplicate_paths_are_rejected_by_schema() {
        let db = Db::open_in_memory().unwrap();
        db.conn
            .execute(
                "INSERT INTO source_root (path, added_at) VALUES ('/p', 1)",
                [],
            )
            .unwrap();
        let insert = "INSERT INTO media_item (source_root_id, path, normalized_path, file_name,
                      extension, media_kind, size_bytes, mtime, scan_timestamp)
                      VALUES (1, '/p/a.jpg', '/p/a.jpg', 'a.jpg', 'jpg', 'photo', 1, 1, 1)";
        db.conn.execute(insert, []).unwrap();
        assert!(db.conn.execute(insert, []).is_err());
    }
}
