use std::sync::Arc;

use axum::extract::{Multipart, State};
use axum::http::StatusCode;
use axum::response::Json;
use axum::routing::post;
use axum::Router;

use super::routes::{vault_relative_path, write_atomically};
use super::AppState;

const HIDDEN_DIRS: &[&str] = &[".catdo", ".git", ".obsidian"];

fn safe_file_name(raw_name: &str) -> Result<String, StatusCode> {
    let normalized = raw_name.replace('\\', "/");
    let name = normalized.rsplit('/').next().unwrap_or("");
    if name.is_empty() || name == "." || name == ".." || name.contains('\0') || normalized != name {
        return Err(StatusCode::BAD_REQUEST);
    }
    Ok(name.to_string())
}

pub fn asset_routes() -> Router<Arc<AppState>> {
    Router::new().route("/assets/upload", post(upload_asset))
}

async fn upload_asset(
    State(state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let mut target_dir: Option<String> = None;
    let mut file_name: Option<String> = None;
    let mut file_data: Option<Vec<u8>> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?
    {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "target_dir" => {
                target_dir = Some(field.text().await.map_err(|_| StatusCode::BAD_REQUEST)?);
            }
            "file" => {
                file_name = field.file_name().map(str::to_string);
                file_data = Some(
                    field
                        .bytes()
                        .await
                        .map_err(|_| StatusCode::BAD_REQUEST)?
                        .to_vec(),
                );
            }
            _ => {}
        }
    }

    let target_dir = target_dir.ok_or(StatusCode::BAD_REQUEST)?;
    let file_name = safe_file_name(&file_name.ok_or(StatusCode::BAD_REQUEST)?)?;
    let file_data = file_data.ok_or(StatusCode::BAD_REQUEST)?;
    if target_dir
        .replace('\\', "/")
        .split('/')
        .any(|segment| HIDDEN_DIRS.contains(&segment))
    {
        return Err(StatusCode::BAD_REQUEST);
    }

    let dest_dir = vault_relative_path(&state.vault_path, &target_dir)?;
    std::fs::create_dir_all(&dest_dir).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let dest_dir = vault_relative_path(&state.vault_path, &target_dir)?;

    let mut destination_name = file_name.clone();
    let lock = state.write_locks.lock_for(&dest_dir);
    let _guard = lock.lock();

    let (stem, extension) = match destination_name.rsplit_once('.') {
        Some((stem, extension)) if !stem.is_empty() => (stem.to_string(), format!(".{extension}")),
        _ => (destination_name.clone(), String::new()),
    };
    let mut suffix = 1u32;
    loop {
        let candidate = dest_dir.join(&destination_name);
        if !candidate.exists() {
            break;
        }
        if std::fs::symlink_metadata(&candidate)
            .map(|metadata| metadata.file_type().is_symlink())
            .unwrap_or(false)
        {
            return Err(StatusCode::BAD_REQUEST);
        }
        destination_name = format!("{stem}_{suffix}{extension}");
        suffix = suffix.saturating_add(1);
    }

    let dest_path = dest_dir.join(&destination_name);
    write_atomically(&dest_path, &file_data)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let relative_path = format!(
        "{}/{}",
        target_dir.replace('\\', "/").trim_end_matches('/'),
        destination_name
    );

    Ok(Json(serde_json::json!({ "relative_path": relative_path })))
}
