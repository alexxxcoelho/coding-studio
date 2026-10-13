//! V2 routes: write-up in docx, anonymised export, whole-project bundle,
//! example project. Every route is a thin adapter over the functions of the
//! `export_v2`, `bundle` and `example` modules, which the command line uses
//! too. Documented in `docs/API.md`, section "V2 exports and CLI".

use axum::extract::State;
use axum::http::{header, HeaderValue};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;

use crate::{ApiError, AppState};

pub(crate) fn routes() -> Router<AppState> {
    Router::new().route("/api/writing/export.docx", get(writeup_docx))
}

fn attachment(bytes: Vec<u8>, content_type: &'static str, file_name: &str) -> Response {
    let mut res = (
        [(header::CONTENT_TYPE, HeaderValue::from_static(content_type))],
        bytes,
    )
        .into_response();
    if let Ok(v) = HeaderValue::from_str(&format!("attachment; filename=\"{file_name}\"")) {
        res.headers_mut().insert(header::CONTENT_DISPOSITION, v);
    }
    res
}

pub(crate) const DOCX_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document";

/// `GET /api/writing/export.docx`: the write-up as a Word document.
async fn writeup_docx(State(st): State<AppState>) -> Result<Response, ApiError> {
    let doc = st.store().writeup_doc()?;
    let bytes = crate::docx::render(&doc)?;
    Ok(attachment(bytes, DOCX_TYPE, "writeup.docx"))
}
