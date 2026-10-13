//! Problems found while reading or planning an import, reported to the user.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// The file or one of its parts is refused.
    Error,
    /// Something is skipped or changed, the import goes on.
    Warning,
}

/// One problem, with where it was found (an XML path, a code name, a source).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Issue {
    pub severity: Severity,
    pub location: String,
    pub message: String,
}

impl Issue {
    pub fn error(location: impl Into<String>, message: impl Into<String>) -> Issue {
        Issue {
            severity: Severity::Error,
            location: location.into(),
            message: message.into(),
        }
    }

    pub fn warning(location: impl Into<String>, message: impl Into<String>) -> Issue {
        Issue {
            severity: Severity::Warning,
            location: location.into(),
            message: message.into(),
        }
    }
}

/// Why an import was refused or failed.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The file is not a usable REFI-QDA file; nothing was written. Every
    /// problem found is listed.
    #[error("file rejected: {} problem(s)", .0.len())]
    Rejected(Vec<Issue>),
    /// The store refused a write or could not be read.
    #[error(transparent)]
    Store(#[from] anyhow::Error),
}

impl Error {
    pub fn rejected(issue: Issue) -> Error {
        Error::Rejected(vec![issue])
    }
}

pub type Result<T> = std::result::Result<T, Error>;
