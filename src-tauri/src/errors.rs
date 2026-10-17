use kadr_core::CoreError;
use serde::Serialize;

/// Error sent to the UI: a stable code and a short Ukrainian message.
/// Technical details go to stderr (the diagnostic log arrives in stage F),
/// never raw into the UI.
#[derive(Debug, Serialize)]
pub struct AppError {
    pub code: &'static str,
    pub message: String,
    /// Which setting was rejected, when relevant.
    pub field: Option<&'static str>,
}

impl AppError {
    pub fn internal(detail: &str) -> Self {
        eprintln!("kadr: internal error: {detail}");
        Self {
            code: "internal",
            message: "Внутрішня помилка. Подробиці записано в журнал.".to_owned(),
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
                    message: invalid_setting_message(field),
                    field: Some(field),
                }
            }
            CoreError::SchemaTooNew { .. } => {
                eprintln!("kadr: {err}");
                Self {
                    code: "schema_too_new",
                    message: "Індекс створено новішою версією Kadr. Оновіть застосунок.".to_owned(),
                    field: None,
                }
            }
            other => Self::internal(&other.to_string()),
        }
    }
}

fn invalid_setting_message(field: &str) -> String {
    match field {
        "unknown_date_label" | "unknown_place_label" => {
            "Назва папки не може бути порожньою, містити символи / \\ : * ? \" < > | \
             чи бути зарезервованою назвою Windows."
                .to_owned()
        }
        "metadata_workers" | "thumbnail_workers" => {
            format!(
                "Кількість потоків має бути від 1 до {}.",
                kadr_core::settings::MAX_WORKERS
            )
        }
        "folder_template" => "Шаблон папок має закінчуватися на {filename}.".to_owned(),
        "network_enabled" => "Мережеві функції в цій версії недоступні.".to_owned(),
        _ => "Недопустиме значення налаштування.".to_owned(),
    }
}
