//! Embedded UI static file serving

use axum::{
    body::Body,
    http::{header, Request, StatusCode},
    response::{Html, IntoResponse, Response},
};
use rust_embed::Embed;

#[derive(Embed)]
#[folder = "ui/dist"]
struct UiAssets;

/// Serve embedded UI static files
pub async fn serve_ui(req: Request<Body>) -> impl IntoResponse {
    let path = req.uri().path().trim_start_matches('/');

    // Try to serve the exact file
    if let Some(response) = serve_file(path) {
        return response;
    }

    // For SPA: if file not found and not an API route, serve index.html
    if !path.starts_with("api/") {
        if let Some(response) = serve_file("index.html") {
            return response;
        }
    }

    // 404
    (StatusCode::NOT_FOUND, "Not Found").into_response()
}

/// Serve a specific embedded file
fn serve_file(path: &str) -> Option<Response> {
    let path = if path.is_empty() { "index.html" } else { path };

    UiAssets::get(path).map(|content| {
        let mime = mime_guess::from_path(path)
            .first_or_octet_stream()
            .to_string();

        Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, mime)
            .header(header::CACHE_CONTROL, cache_control(path))
            .body(Body::from(content.data.into_owned()))
            .unwrap()
    })
}

/// Determine cache control based on file type
fn cache_control(path: &str) -> &'static str {
    if path == "index.html" {
        // Don't cache index.html to ensure users get updates
        "no-cache, no-store, must-revalidate"
    } else if path.contains("/static/") || path.ends_with(".js") || path.ends_with(".css") {
        // Cache static assets with hash in filename for 1 year
        "public, max-age=31536000, immutable"
    } else {
        // Cache other assets for 1 hour
        "public, max-age=3600"
    }
}

/// Serve index.html for the root path
pub async fn serve_index() -> impl IntoResponse {
    match UiAssets::get("index.html") {
        Some(content) => Html(content.data.into_owned()).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            "UI not built. Run: cd ui && npm run build",
        )
            .into_response(),
    }
}
