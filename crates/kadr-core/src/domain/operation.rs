use serde::{Deserialize, Serialize};

/// How a plan is applied. Copy is the safe default; Move must be chosen
/// explicitly for every plan and is never remembered as a default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationMode {
    #[default]
    Copy,
    Move,
}

/// What to do when the destination path is already taken. Overwriting is not
/// an option at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictPolicy {
    /// Append ` (1)`, ` (2)`, ... before the extension.
    #[default]
    AddSuffix,
    Skip,
    AskUser,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationStatus {
    Planned,
    Excluded,
    InProgress,
    Done,
    Failed,
    Skipped,
    Undone,
    /// The app closed while this item was in progress.
    Interrupted,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_defaults() {
        assert_eq!(OperationMode::default(), OperationMode::Copy);
        assert_eq!(ConflictPolicy::default(), ConflictPolicy::AddSuffix);
    }
}
