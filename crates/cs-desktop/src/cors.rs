//! CORS for the embedded server.
//!
//! In the desktop app the UI is not served by the API server: it comes from the
//! Tauri asset protocol (`tauri://localhost` on macOS, `http://tauri.localhost`
//! on Windows) or, under `cargo tauri dev`, from the Vite dev server. The page
//! therefore calls the loopback API cross-origin. Only those origins may read
//! the answers; any other site, and the `coding-studio` binary used from a
//! browser (same origin), are untouched.

use axum::extract::Request;
use axum::http::{header, HeaderValue, Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

/// Origins of the UI in a packaged app.
const APP_ORIGINS: [&str; 2] = ["tauri://localhost", "http://tauri.localhost"];
/// Vite dev server (`cargo tauri dev`), allowed in debug builds only.
const DEV_ORIGIN: &str = "http://127.0.0.1:5199";

fn allowed(origin: &str, dev: bool) -> bool {
    APP_ORIGINS.contains(&origin) || (dev && origin == DEV_ORIGIN)
}

/// Answers preflights and adds the CORS headers for the allowed origins.
pub async fn layer(req: Request, next: Next) -> Response {
    let origin = req
        .headers()
        .get(header::ORIGIN)
        .and_then(|v| v.to_str().ok())
        .filter(|o| allowed(o, cfg!(debug_assertions)))
        .and_then(|o| HeaderValue::from_str(o).ok());
    let preflight = req.method() == Method::OPTIONS && origin.is_some();
    let mut res = if preflight {
        StatusCode::NO_CONTENT.into_response()
    } else {
        next.run(req).await
    };
    if let Some(origin) = origin {
        let h = res.headers_mut();
        h.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, origin);
        h.insert(header::VARY, HeaderValue::from_static("Origin"));
        if preflight {
            h.insert(
                header::ACCESS_CONTROL_ALLOW_METHODS,
                HeaderValue::from_static("GET, POST, PUT, DELETE, OPTIONS"),
            );
            h.insert(
                header::ACCESS_CONTROL_ALLOW_HEADERS,
                HeaderValue::from_static("content-type"),
            );
            h.insert(
                header::ACCESS_CONTROL_MAX_AGE,
                HeaderValue::from_static("600"),
            );
        }
    }
    res
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::middleware::from_fn;
    use axum::routing::get;
    use axum::Router;
    use tower::ServiceExt;

    fn app() -> Router {
        Router::new()
            .route("/api/state", get(|| async { "ok" }))
            .layer(from_fn(layer))
    }

    async fn send(method: &str, origin: Option<&str>) -> Response {
        let mut req = Request::builder().method(method).uri("/api/state");
        if let Some(o) = origin {
            req = req.header(header::ORIGIN, o);
        }
        app()
            .oneshot(req.body(Body::empty()).unwrap())
            .await
            .unwrap()
    }

    #[test]
    fn only_the_tauri_origins_are_allowed() {
        assert!(allowed("tauri://localhost", false));
        assert!(allowed("http://tauri.localhost", false));
        assert!(!allowed(DEV_ORIGIN, false));
        assert!(allowed(DEV_ORIGIN, true));
        assert!(!allowed("https://example.org", true));
        assert!(!allowed("http://127.0.0.1:7878", true));
    }

    #[tokio::test]
    async fn preflight_of_an_app_origin_is_answered_without_reaching_the_route() {
        let res = send("OPTIONS", Some("tauri://localhost")).await;
        assert_eq!(res.status(), StatusCode::NO_CONTENT);
        assert_eq!(
            res.headers()[header::ACCESS_CONTROL_ALLOW_ORIGIN],
            "tauri://localhost"
        );
        assert!(res
            .headers()
            .contains_key(header::ACCESS_CONTROL_ALLOW_METHODS));
    }

    #[tokio::test]
    async fn answers_to_an_app_origin_carry_the_header_others_do_not() {
        let res = send("GET", Some("http://tauri.localhost")).await;
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(
            res.headers()[header::ACCESS_CONTROL_ALLOW_ORIGIN],
            "http://tauri.localhost"
        );
        let res = send("GET", Some("https://example.org")).await;
        assert_eq!(res.status(), StatusCode::OK);
        assert!(!res
            .headers()
            .contains_key(header::ACCESS_CONTROL_ALLOW_ORIGIN));
        let res = send("GET", None).await;
        assert!(!res
            .headers()
            .contains_key(header::ACCESS_CONTROL_ALLOW_ORIGIN));
    }
}
