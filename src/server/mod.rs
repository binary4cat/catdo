pub mod assets;
pub mod embed;
pub mod routes;
pub mod ws;

use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::Router;

use crate::watcher::WatcherState;
use routes::IdempotencyCache;
#[derive(Clone)]
pub struct AppState {
    pub vault_path: PathBuf,
    pub watcher: WatcherState,
    pub idempotency: Arc<IdempotencyCache>,
    pub write_locks: Arc<routes::WriteLocks>,
}

async fn same_origin(req: Request<Body>, next: Next) -> Response {
    let cross_site = req
        .headers()
        .get("sec-fetch-site")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.eq_ignore_ascii_case("cross-site"));
    let allowed_origin = req
        .headers()
        .get("origin")
        .and_then(|value| value.to_str().ok())
        .map(|origin| {
            let host = req
                .headers()
                .get("host")
                .and_then(|value| value.to_str().ok())
                .unwrap_or("");
            origin == format!("http://{host}") || origin == format!("https://{host}")
        })
        .unwrap_or(true);
    if cross_site || !allowed_origin {
        return (StatusCode::FORBIDDEN, "cross-origin request rejected").into_response();
    }
    next.run(req).await
}


pub fn build_router(vault_path: PathBuf, watcher: WatcherState) -> Router {
    let state = Arc::new(AppState {
        vault_path: vault_path.clone(),
        watcher,
        idempotency: Arc::new(IdempotencyCache::new(&vault_path)),
        write_locks: Arc::new(routes::WriteLocks::new()),
    });


    let api_routes = routes::api_routes()
        .merge(assets::asset_routes())
        .merge(ws::ws_routes());

    Router::new()
        .nest("/api", api_routes)
        .merge(routes::raw_routes())
        .fallback(embed::static_handler)
        .layer(middleware::from_fn(same_origin))
        .with_state(state)
}
