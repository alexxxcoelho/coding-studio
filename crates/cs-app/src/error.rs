//! Maps library errors onto HTTP answers the UI can show as they are.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use cs_model::{CsError, ValidationIssue};
use serde_json::json;

/// Error returned by every API route: `{"error", "kind", "issues"?}`.
#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub kind: &'static str,
    pub message: String,
    pub issues: Option<Vec<ValidationIssue>>,
}

impl ApiError {
    /// AI answer rejected: every problem found is returned, not only the first.
    pub fn with_issues(issues: Vec<ValidationIssue>) -> ApiError {
        ApiError {
            status: StatusCode::BAD_REQUEST,
            kind: "validation",
            message: format!(
                "the AI answer was rejected: {} problem(s) found",
                issues.len()
            ),
            issues: Some(issues),
        }
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(err: anyhow::Error) -> ApiError {
        // The protocol error may sit anywhere in the context chain.
        let cs = err.chain().find_map(|e| e.downcast_ref::<CsError>());
        let (status, kind) = match cs {
            Some(CsError::Validation(_)) => (StatusCode::BAD_REQUEST, "validation"),
            Some(CsError::Guard(_)) => (StatusCode::CONFLICT, "guard"),
            Some(CsError::NotFound(_)) => (StatusCode::NOT_FOUND, "not_found"),
            None => (StatusCode::INTERNAL_SERVER_ERROR, "internal"),
        };
        ApiError {
            status,
            kind,
            message: format!("{err:#}"),
            issues: None,
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let mut body = json!({ "error": self.message, "kind": self.kind });
        if let Some(issues) = self.issues {
            body["issues"] = json!(issues);
        }
        (self.status, Json(body)).into_response()
    }
}
