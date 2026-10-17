//! Folder naming. The template parser and path sanitizer arrive in stage D;
//! this module holds the pieces the settings already need.

use serde::{Deserialize, Serialize};

/// Default destination layout: `{year}/{month}/{place}/{filename}`, where
/// `{month}` is rendered according to [`MonthFormat`].
pub const DEFAULT_TEMPLATE: &str = "{year}/{month}/{place}/{filename}";

const MONTH_NAMES_UK: [&str; 12] = [
    "Січень",
    "Лютий",
    "Березень",
    "Квітень",
    "Травень",
    "Червень",
    "Липень",
    "Серпень",
    "Вересень",
    "Жовтень",
    "Листопад",
    "Грудень",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MonthFormat {
    /// `08 - Серпень` — keeps folders in chronological order.
    #[default]
    NumberName,
    /// `08`
    Number,
    /// `Серпень`
    Name,
}

/// Ukrainian month name for 1..=12, `None` otherwise.
pub fn month_name_uk(month: u32) -> Option<&'static str> {
    let index = usize::try_from(month.checked_sub(1)?).ok()?;
    MONTH_NAMES_UK.get(index).copied()
}

impl MonthFormat {
    /// Folder name for a month number 1..=12, `None` for anything else.
    pub fn render(self, month: u32) -> Option<String> {
        let name = month_name_uk(month)?;
        Some(match self {
            Self::NumberName => format!("{month:02} - {name}"),
            Self::Number => format!("{month:02}"),
            Self::Name => name.to_owned(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_all_month_formats() {
        assert_eq!(
            MonthFormat::NumberName.render(8).as_deref(),
            Some("08 - Серпень")
        );
        assert_eq!(MonthFormat::Number.render(8).as_deref(), Some("08"));
        assert_eq!(MonthFormat::Name.render(12).as_deref(), Some("Грудень"));
    }

    #[test]
    fn rejects_invalid_month_numbers() {
        assert_eq!(MonthFormat::Number.render(0), None);
        assert_eq!(MonthFormat::Number.render(13), None);
    }

    #[test]
    fn number_name_folders_sort_chronologically() {
        let mut names: Vec<String> = (1..=12)
            .filter_map(|m| MonthFormat::NumberName.render(m))
            .collect();
        let chronological = names.clone();
        names.sort();
        assert_eq!(names, chronological);
    }
}
