use std::sync::Arc;

use axum::extract::State;
use axum::http::{header, StatusCode, Uri};
use axum::response::Response;
use rust_embed::Embed;

use super::AppState;

#[derive(Embed)]
#[folder = "web/dist"]
struct FrontendAssets;

pub async fn static_handler(
    State(_state): State<Arc<AppState>>,
    uri: Uri,
) -> Result<Response, StatusCode> {
    let path = uri.path().trim_start_matches('/');

    // API and raw file paths must never fall back to index.html. A malformed
    // or traversal-looking request should be a hard 404, not an HTML 200.
    if path == "api" || path.starts_with("api/") || path == "raw" || path.starts_with("raw/") {
        return Err(StatusCode::NOT_FOUND);
    }

    if let Some(content) = FrontendAssets::get(path) {
        return build_response(path, &content.data);
    }

    if let Some(content) = FrontendAssets::get("index.html") {
        return build_response("index.html", &content.data);
    }

    Err(StatusCode::NOT_FOUND)
}

fn build_response(path: &str, data: &[u8]) -> Result<Response, StatusCode> {
    let mime = mime_guess::from_path(path)
        .first_or_octet_stream()
        .to_string();

    Response::builder()
        .header(header::CONTENT_TYPE, mime)
        .body(axum::body::Body::from(data.to_vec()))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}
