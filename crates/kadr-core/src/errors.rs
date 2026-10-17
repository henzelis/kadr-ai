use thiserror::Error;

/// Errors raised by the core. Technical details stay here; the shell maps them
/// to a short user-facing message and keeps the details for the diagnostic log.
#[derive(Debug, Error)]
pub enum CoreError {
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("invalid setting `{field}`: {reason}")]
    InvalidSetting { field: &'static str, reason: String },

    #[error("invalid geobase: {0}")]
    InvalidGeobase(String),

    #[error("database schema version {found} is newer than this build supports ({supported})")]
    SchemaTooNew { found: i64, supported: i64 },
}

pub type CoreResult<T> = Result<T, CoreError>;
