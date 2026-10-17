use kadr_core::CoreError;
use serde::Serialize;

/// Error sent to the UI: a stable code (translated by the UI), the setting
/// or value it concerns, and parameters. Technical details go to stderr
/// (the diagnostic log arrives in stage F), never raw into the UI.
#[derive(Debug, Serialize)]
pub struct AppError {
    pub code: &'static str,
    pub field: Option<&'static str>,
}

impl AppError {
    /// A problem the user can act on; the UI explains it by `code`.
    pub fn user(code: &'static str) -> Self {
        Self { code, field: None }
    }

    pub fn internal(detail: &str) -> Self {
        eprintln!("kadr: internal error: {detail}");
        Self {
            code: "internal",
            field: None,
        }
    }
}

impl From<CoreError> for AppError {
    fn from(err: CoreError) -> Self {
        match err {
            CoreError::InvalidSetting { field, reason } => {
                eprintln!("kadr: invalid setting {field}: {reason}");
                Self {
                    code: "invalid_setting",
                    field: Some(field),
                }
            }
            CoreError::InvalidValue { field, reason } => {
                eprintln!("kadr: invalid value {field}: {reason}");
                Self {
                    code: "invalid_value",
                    field: Some(field),
                }
            }
            CoreError::SchemaTooNew { .. } => {
                eprintln!("kadr: {err}");
                Self::user("schema_too_new")
            }
            other => Self::internal(&other.to_string()),
        }
    }
}
