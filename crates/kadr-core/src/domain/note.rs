//! Messages for the user are stored and sent as a code plus parameters; the
//! UI turns them into text in the user's language. The core never builds
//! sentences in a particular language.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Note {
    pub code: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub args: BTreeMap<String, String>,
}

impl Note {
    pub fn new(code: &str) -> Self {
        Self {
            code: code.to_owned(),
            args: BTreeMap::new(),
        }
    }

    pub fn arg(mut self, key: &str, value: impl ToString) -> Self {
        self.args.insert(key.to_owned(), value.to_string());
        self
    }

    /// Reads a stored note; plain text from older versions becomes `text`.
    pub fn parse(stored: &str) -> Self {
        serde_json::from_str(stored).unwrap_or_else(|_| Self::new("text").arg("text", stored))
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_legacy_text() {
        let n = Note::new("date_conflict").arg("a", "2020").arg("b", "2024");
        assert_eq!(Note::parse(&n.to_json()), n);
        assert_eq!(Note::parse("Стара примітка").args["text"], "Стара примітка");
        assert_eq!(Note::new("gps_zero").to_json(), r#"{"code":"gps_zero"}"#);
    }
}
