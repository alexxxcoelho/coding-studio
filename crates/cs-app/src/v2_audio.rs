//! F-62 routes: audio linked to the transcript. The recordings of an interview
//! folder, served with HTTP range requests from a route that reaches nothing
//! but the audio files of that folder; the start time of each unit in seconds
//! with the researcher's offset applied; the offset itself. No transcription.
//! Documented in `docs/API.md`, section "V2 transcripts and audio".

use std::io::SeekFrom;
use std::path::{Path, PathBuf};

use axum::body::Body;
use axum::extract::{Path as UrlPath, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, put};
use axum::{Json, Router};
use cs_ingest::{find_audio, timestamp_seconds, AudioFile};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio_util::io::ReaderStream;

use crate::{invalid, not_found, require_interview, ApiError, ApiResult, AppState};

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/interviews/{id}/audio", get(audio_map))
        .route("/api/interviews/{id}/audio/offset", put(set_offset))
        .route("/api/interviews/{id}/audio/file", get(audio_file))
}

// ------------------------------------------------------------------ ranges

/// What a `Range` header asks for, against a file of `total` bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RangeSpec {
    /// No usable range: the whole file.
    Whole,
    /// Bytes `start..=end`.
    Part(u64, u64),
    /// The range starts past the end of the file.
    Unsatisfiable,
}

/// Reads a `Range: bytes=...` header. Only the first range of a list is
/// served; a header that is not a byte range, or is malformed, is ignored (the
/// whole file is sent), as RFC 9110 allows.
pub(crate) fn parse_range(header: Option<&str>, total: u64) -> RangeSpec {
    let Some(spec) = header.and_then(|h| h.trim().strip_prefix("bytes=")) else {
        return RangeSpec::Whole;
    };
    let first = spec.split(',').next().unwrap_or("").trim();
    let Some((a, b)) = first.split_once('-') else {
        return RangeSpec::Whole;
    };
    let (a, b) = (a.trim(), b.trim());
    match (a.is_empty(), b.is_empty()) {
        (true, true) => RangeSpec::Whole,
        (true, false) => {
            let Ok(n) = b.parse::<u64>() else {
                return RangeSpec::Whole;
            };
            if n == 0 || total == 0 {
                return RangeSpec::Unsatisfiable;
            }
            RangeSpec::Part(total.saturating_sub(n), total - 1)
        }
        (false, true) => {
            let Ok(start) = a.parse::<u64>() else {
                return RangeSpec::Whole;
            };
            if start >= total {
                RangeSpec::Unsatisfiable
            } else {
                RangeSpec::Part(start, total - 1)
            }
        }
        (false, false) => {
            let (Ok(start), Ok(end)) = (a.parse::<u64>(), b.parse::<u64>()) else {
                return RangeSpec::Whole;
            };
            if start > end {
                RangeSpec::Whole
            } else if start >= total {
                RangeSpec::Unsatisfiable
            } else {
                RangeSpec::Part(start, end.min(total - 1))
            }
        }
    }
}

// ------------------------------------------------------------------- safety

/// The file of `rel` among the audio files of an interview, resolved to a real
/// path that lies inside the interview folder and, when the library folder is
/// known, inside the library. A name that is not in the list of audio files
/// (a transcript, a `..` path, an absolute path, a link leading elsewhere) is
/// refused.
fn resolve_audio(
    folder: &str,
    library_root: Option<&str>,
    files: &[AudioFile],
    rel: &str,
) -> Result<PathBuf, ApiError> {
    if rel.is_empty()
        || rel.contains('\\')
        || rel.contains('\0')
        || rel.starts_with('/')
        || rel.split('/').any(|s| s == ".." || s == ".")
    {
        return Err(invalid("path must stay inside the interview folder"));
    }
    if !files.iter().any(|f| f.rel_path == rel) {
        return Err(not_found(format!(
            "{rel} is not an audio file of this interview"
        )));
    }
    let gone = |_| not_found(format!("{rel} is no longer in the interview folder"));
    let folder = Path::new(folder).canonicalize().map_err(gone)?;
    if let Some(root) = library_root {
        let root = Path::new(root).canonicalize().map_err(gone)?;
        if !folder.starts_with(&root) {
            return Err(invalid("the interview folder is outside the library"));
        }
    }
    let real = folder.join(rel).canonicalize().map_err(gone)?;
    if !real.starts_with(&folder) || !real.is_file() {
        return Err(invalid("path must stay inside the interview folder"));
    }
    Ok(real)
}

fn url_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

// ------------------------------------------------------------------- routes

#[derive(Serialize)]
struct AudioUnit {
    unit_id: String,
    seq: u32,
    role: cs_model::SpeakerRole,
    timestamp: String,
    /// Seconds read from the transcript timestamp, `None` when the stamp is not a time.
    start_seconds: Option<f64>,
    /// Where to seek in the recording: start plus the interview's offset, never below 0.
    audio_seconds: Option<f64>,
    /// Audio time of the next timed unit, `None` for the last one.
    end_seconds: Option<f64>,
}

fn round_ms(x: f64) -> f64 {
    (x * 1000.0).round() / 1000.0
}

fn unit_times(units: &[cs_model::Unit], offset: f64) -> Vec<AudioUnit> {
    let mut out: Vec<AudioUnit> = units
        .iter()
        .map(|u| {
            let start = timestamp_seconds(&u.timestamp);
            AudioUnit {
                unit_id: u.unit_id.clone(),
                seq: u.seq,
                role: u.role,
                timestamp: u.timestamp.clone(),
                start_seconds: start.map(round_ms),
                audio_seconds: start.map(|s| round_ms((s + offset).max(0.0))),
                end_seconds: None,
            }
        })
        .collect();
    let mut next: Option<f64> = None;
    for u in out.iter_mut().rev() {
        u.end_seconds = next;
        if u.audio_seconds.is_some() {
            next = u.audio_seconds;
        }
    }
    out
}

/// Recordings, offset and unit times of one interview, for the player.
async fn audio_map(State(st): State<AppState>, UrlPath(id): UrlPath<String>) -> ApiResult {
    let (entry, offset, seg) = {
        let store = st.store();
        let entry = require_interview(&store, &id)?;
        let offset = store.audio_offset(&id)?;
        let seg = store.segmentation(&id)?;
        (entry, offset, seg)
    };
    let files = find_audio(Path::new(&entry.folder_path))?;
    let listed: Vec<_> = files
        .iter()
        .map(|f| {
            json!({
                "rel_path": f.rel_path,
                "size": f.size,
                "mime": f.mime,
                "url": format!(
                    "/api/interviews/{id}/audio/file?path={}",
                    url_encode(&f.rel_path)
                ),
            })
        })
        .collect();
    let units = seg
        .map(|s| unit_times(&s.units, offset.offset_seconds))
        .unwrap_or_default();
    let timed = units.iter().filter(|u| u.start_seconds.is_some()).count();
    let mut warnings = Vec::new();
    if files.is_empty() {
        warnings.push("no audio file (m4a, mp3, wav) in the interview folder".to_string());
    }
    if files.len() > 1 {
        warnings.push(format!(
            "{} audio files found; the offset applies to the interview, not to a file",
            files.len()
        ));
    }
    if !units.is_empty() && timed == 0 {
        warnings.push("no unit carries a time; the transcript cannot be synchronised".to_string());
    } else if timed < units.len() {
        warnings.push(format!(
            "{} of {} units carry no time",
            units.len() - timed,
            units.len()
        ));
    }
    let backwards = units
        .iter()
        .filter_map(|u| u.start_seconds)
        .collect::<Vec<_>>()
        .windows(2)
        .filter(|w| w[1] < w[0])
        .count();
    if backwards > 0 {
        warnings.push(format!(
            "{backwards} unit(s) start earlier than the unit before; check the timestamps"
        ));
    }
    Ok(Json(json!({
        "interview_id": id,
        "files": listed,
        "has_audio": !files.is_empty(),
        "offset": offset,
        "timed_units": timed,
        "units": units,
        "warnings": warnings,
    })))
}

#[derive(Deserialize)]
struct OffsetBody {
    offset_seconds: f64,
    #[serde(default)]
    note: String,
}

async fn set_offset(
    State(st): State<AppState>,
    UrlPath(id): UrlPath<String>,
    Json(body): Json<OffsetBody>,
) -> ApiResult {
    let offset = st
        .store()
        .set_audio_offset(&id, body.offset_seconds, &body.note, &st.actor)?;
    Ok(Json(json!(offset)))
}

#[derive(Deserialize, Default)]
struct FileQuery {
    #[serde(default)]
    path: Option<String>,
}

fn header_str(headers: &HeaderMap, name: header::HeaderName) -> Option<&str> {
    headers.get(name).and_then(|v| v.to_str().ok())
}

/// One recording, with `Range` support (206 and `Content-Range`, 416 past the
/// end). `?path=` is the `rel_path` listed by the audio map; without it the
/// first recording is served.
async fn audio_file(
    State(st): State<AppState>,
    UrlPath(id): UrlPath<String>,
    Query(q): Query<FileQuery>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let (folder, root) = {
        let store = st.store();
        let entry = require_interview(&store, &id)?;
        (entry.folder_path, store.setting("library_root")?)
    };
    let files = find_audio(Path::new(&folder))?;
    let rel = match q.path.as_deref().filter(|p| !p.is_empty()) {
        Some(p) => p.to_string(),
        None => files
            .first()
            .map(|f| f.rel_path.clone())
            .ok_or_else(|| not_found(format!("no audio file in the folder of {id}")))?,
    };
    let mime = files
        .iter()
        .find(|f| f.rel_path == rel)
        .map(|f| f.mime.clone())
        .unwrap_or_else(|| "application/octet-stream".into());
    let real = resolve_audio(&folder, root.as_deref(), &files, &rel)?;
    let mut file = tokio::fs::File::open(&real)
        .await
        .map_err(|e| invalid(format!("cannot read {rel}: {e}")))?;
    let total = file
        .metadata()
        .await
        .map_err(|e| invalid(format!("cannot read {rel}: {e}")))?
        .len();
    let range = parse_range(header_str(&headers, header::RANGE), total);
    let base = Response::builder()
        .header(header::CONTENT_TYPE, mime)
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CACHE_CONTROL, "private, no-cache");
    let response = match range {
        RangeSpec::Unsatisfiable => base
            .status(StatusCode::RANGE_NOT_SATISFIABLE)
            .header(header::CONTENT_RANGE, format!("bytes */{total}"))
            .body(Body::empty()),
        RangeSpec::Whole => base
            .status(StatusCode::OK)
            .header(header::CONTENT_LENGTH, total)
            .body(Body::from_stream(ReaderStream::with_capacity(
                file,
                64 * 1024,
            ))),
        RangeSpec::Part(start, end) => {
            file.seek(SeekFrom::Start(start))
                .await
                .map_err(|e| invalid(format!("cannot read {rel}: {e}")))?;
            let len = end - start + 1;
            base.status(StatusCode::PARTIAL_CONTENT)
                .header(
                    header::CONTENT_RANGE,
                    format!("bytes {start}-{end}/{total}"),
                )
                .header(header::CONTENT_LENGTH, len)
                .body(Body::from_stream(ReaderStream::with_capacity(
                    file.take(len),
                    64 * 1024,
                )))
        }
    };
    response
        .map(IntoResponse::into_response)
        .map_err(|e| invalid(format!("cannot build the response: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges_are_read_as_rfc_9110_says() {
        let r = |h: &str| parse_range(Some(h), 1000);
        assert_eq!(parse_range(None, 1000), RangeSpec::Whole);
        assert_eq!(r("bytes=0-9"), RangeSpec::Part(0, 9));
        assert_eq!(r("bytes=990-"), RangeSpec::Part(990, 999));
        assert_eq!(r("bytes=-5"), RangeSpec::Part(995, 999));
        assert_eq!(r("bytes=-5000"), RangeSpec::Part(0, 999));
        assert_eq!(r("bytes=900-5000"), RangeSpec::Part(900, 999));
        assert_eq!(r("bytes=0-0"), RangeSpec::Part(0, 0));
        assert_eq!(r("bytes=0-9, 20-29"), RangeSpec::Part(0, 9));
        assert_eq!(r("bytes=1000-"), RangeSpec::Unsatisfiable);
        assert_eq!(r("bytes=1500-1600"), RangeSpec::Unsatisfiable);
        assert_eq!(r("bytes=-0"), RangeSpec::Unsatisfiable);
        assert_eq!(r("bytes=9-0"), RangeSpec::Whole);
        assert_eq!(r("bytes=abc"), RangeSpec::Whole);
        assert_eq!(r("bytes=-"), RangeSpec::Whole);
        assert_eq!(r("items=0-5"), RangeSpec::Whole);
        assert_eq!(parse_range(Some("bytes=0-"), 0), RangeSpec::Unsatisfiable);
    }

    #[test]
    fn paths_are_url_encoded() {
        assert_eq!(
            url_encode("sub/Entretien n°1 é.m4a"),
            "sub/Entretien%20n%C2%B01%20%C3%A9.m4a"
        );
    }
}
