use std::cmp::Reverse;
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::Json;
use axum::routing::get;
use axum::Router;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::AppState;
use crate::config;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FileNode {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: u64,
    pub children: Option<Vec<FileNode>>,
}

#[derive(Deserialize)]
pub struct FileQuery {
    pub path: String,
}

#[derive(Deserialize, Default)]
pub struct TreeQuery {
    pub path: Option<String>,
}

#[derive(Deserialize)]
pub struct SearchQuery {
    pub query: String,
}

#[derive(Serialize)]
pub struct FileContent {
    pub path: String,
    pub content: String,
    pub modified: u64,
    pub version: String,
}

#[derive(Serialize, Deserialize)]
pub struct FileSave {
    pub path: String,
    pub content: String,
    #[serde(default)]
    pub expected_version: Option<String>,
    #[serde(default, skip_serializing)]
    pub request_id: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct FilePatch {
    pub path: String,
    pub base_version: String,
    pub operations: Vec<PatchOp>,
    #[serde(default, skip_serializing)]
    pub request_id: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "op")]
pub enum PatchOp {
    #[serde(rename = "replace_lines")]
    ReplaceLines {
        start_line: usize,
        end_line: usize,
        #[serde(default)]
        expected_hash: Option<String>,
        text: String,
    },
    #[serde(rename = "insert_before")]
    InsertBefore {
        line: usize,
        #[serde(default)]
        anchor_hash: Option<String>,
        text: String,
    },
    #[serde(rename = "insert_after")]
    InsertAfter {
        line: usize,
        #[serde(default)]
        anchor_hash: Option<String>,
        text: String,
    },
    #[serde(rename = "delete_lines")]
    DeleteLines {
        start_line: usize,
        end_line: usize,
        #[serde(default)]
        expected_hash: Option<String>,
    },
    #[serde(rename = "append")]
    Append {
        #[serde(default)]
        expected_tail_hash: Option<String>,
        text: String,
    },
}

const HIDDEN_DIRS: &[&str] = &[".catdo", ".git", ".obsidian"];
const ASSET_DIR: &str = "_assets";

fn is_hidden_dir(name: &str) -> bool {
    HIDDEN_DIRS.iter().any(|hidden| {
        if cfg!(windows) {
            name.eq_ignore_ascii_case(hidden)
        } else {
            name == *hidden
        }
    })
}

fn is_reserved_windows_name(name: &str) -> bool {
    if !cfg!(windows) {
        return false;
    }
    let trimmed = name.trim_end_matches([' ', '.']);
    if trimmed != name {
        return true;
    }
    let stem = trimmed.split('.').next().unwrap_or("").to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.as_bytes()[3].is_ascii_digit()
            && stem.as_bytes()[3] != b'0')
}

fn should_skip(name: &str) -> bool {
    is_hidden_dir(name) || name == ASSET_DIR
}

fn json_error(status: StatusCode, error: &str) -> (StatusCode, Json<Value>) {
    (status, Json(serde_json::json!({ "error": error })))
}

fn path_has_symlink_or_escape(vault_root: &Path, relative: &Path) -> bool {
    let canonical_root = match std::fs::canonicalize(vault_root) {
        Ok(path) => path,
        Err(_) => return true,
    };
    let mut current = canonical_root.clone();
    for component in relative.components() {
        let Component::Normal(name) = component else {
            return true;
        };
        current.push(name);
        match std::fs::symlink_metadata(&current) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() {
                    return true;
                }
                if let Ok(canonical) = std::fs::canonicalize(&current) {
                    if !canonical.starts_with(&canonical_root) {
                        return true;
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(_) => return true,
        }
    }
    false
}

pub(crate) fn vault_relative_path(
    vault_root: &Path,
    raw_path: &str,
) -> Result<PathBuf, StatusCode> {
    let normalized = raw_path.replace('\\', "/");
    if normalized.is_empty() || normalized.contains('\0') {
        return Err(StatusCode::BAD_REQUEST);
    }
    if Path::new(&normalized).is_absolute() || normalized.starts_with('/') {
        return Err(StatusCode::BAD_REQUEST);
    }

    let mut relative = PathBuf::new();
    for segment in normalized.split('/') {
        if segment.is_empty() || segment == "." {
            continue;
        }
        if segment == ".." || is_hidden_dir(segment) || is_reserved_windows_name(segment) {
            return Err(StatusCode::BAD_REQUEST);
        }
        if cfg!(windows) && segment.contains(':') {
            return Err(StatusCode::BAD_REQUEST);
        }
        relative.push(segment);
    }
    if relative.as_os_str().is_empty() || path_has_symlink_or_escape(vault_root, &relative) {
        return Err(StatusCode::BAD_REQUEST);
    }
    Ok(vault_root.join(relative))
}

pub struct WriteLocks {
    entries: Mutex<HashMap<String, Arc<Mutex<()>>>>,
}
impl WriteLocks {
    pub fn new() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
        }
    }

    pub fn lock_for(&self, path: &Path) -> Arc<Mutex<()>> {
        let mut key = path.to_string_lossy().replace('\\', "/");
        if cfg!(windows) {
            key.make_ascii_lowercase();
        }
        self.entries
            .lock()
            .entry(key)
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }
}
#[derive(Clone, Serialize, Deserialize)]
struct IdempotencyRecord {
    fingerprint: String,
    response: Value,
}

#[derive(Serialize, Deserialize)]
struct PersistedIdempotencyRecord {
    request_id: String,
    fingerprint: String,
    response: Value,
}

pub struct IdempotencyCache {
    entries: Mutex<HashMap<String, IdempotencyRecord>>,
    request_locks: Mutex<HashMap<String, Arc<Mutex<()>>>>,
    persistence_path: PathBuf,
}

impl IdempotencyCache {
    pub fn new(vault_path: &Path) -> Self {
        let persistence_path = vault_path.join(".catdo/idempotency.json");
        let mut entries = HashMap::new();
        if let Ok(content) = std::fs::read_to_string(&persistence_path) {
            if let Ok(records) = serde_json::from_str::<Vec<PersistedIdempotencyRecord>>(&content) {
                for record in records.into_iter().rev().take(256) {
                    entries.insert(
                        record.request_id,
                        IdempotencyRecord {
                            fingerprint: record.fingerprint,
                            response: record.response,
                        },
                    );
                }
            }
        }
        Self {
            entries: Mutex::new(entries),
            request_locks: Mutex::new(HashMap::new()),
            persistence_path,
        }
    }

    fn lock_for(&self, request_id: &str) -> Arc<Mutex<()>> {
        self.request_locks
            .lock()
            .entry(request_id.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }

    fn check(&self, request_id: &str, fingerprint: &str) -> Result<Option<Value>, ()> {
        let entries = self.entries.lock();
        match entries.get(request_id) {
            None => Ok(None),
            Some(record) if record.fingerprint == fingerprint => {
                let mut response = record.response.clone();
                if let Value::Object(object) = &mut response {
                    object.insert("replayed".into(), Value::Bool(true));
                }
                Ok(Some(response))
            }
            Some(_) => Err(()),
        }
    }

    fn record(&self, request_id: String, fingerprint: String, response: Value) {
        let mut entries = self.entries.lock();
        if entries.len() >= 256 {
            if let Some(key) = entries.keys().next().cloned() {
                entries.remove(&key);
            }
        }
        entries.insert(
            request_id,
            IdempotencyRecord {
                fingerprint,
                response,
            },
        );

        let persisted: Vec<_> = entries
            .iter()
            .map(|(request_id, record)| PersistedIdempotencyRecord {
                request_id: request_id.clone(),
                fingerprint: record.fingerprint.clone(),
                response: record.response.clone(),
            })
            .collect();
        if let Ok(bytes) = serde_json::to_vec(&persisted) {
            let _ = write_atomically(&self.persistence_path, &bytes);
        }
    }
}

fn content_version(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("sha256:{:x}", hasher.finalize())
}

fn short_hash(bytes: impl AsRef<[u8]>) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let hash = hasher.finalize();
    hash[..8]
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect()
}
fn line_hash(text: &str) -> String {
    short_hash(text.as_bytes())
}

fn lines_hash(lines: &[String], start: usize, end: usize) -> String {
    let text = lines[start - 1..end].join("\n");
    short_hash(text.as_bytes())
}

pub(crate) fn write_atomically(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "missing parent"))?;
    let existing_permissions = match std::fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || metadata.permissions().readonly() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "target is a symlink or read-only file",
                ));
            }
            Some(metadata.permissions())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error),
    };
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("file");
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temp_path = parent.join(format!(".{name}.catdo-{}-{stamp}.tmp", std::process::id()));

    let result = (|| {
        let mut temp = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)?;
        if let Some(permissions) = existing_permissions {
            temp.set_permissions(permissions)?;
        }
        temp.write_all(contents)?;
        temp.sync_all()?;
        drop(temp);
        replace_file(&temp_path, path)
    })();

    if result.is_err() {
        let _ = std::fs::remove_file(&temp_path);
    }
    result
}

#[cfg(not(windows))]
fn replace_file(temp_path: &Path, target_path: &Path) -> std::io::Result<()> {
    std::fs::rename(temp_path, target_path)
}

#[cfg(windows)]
fn replace_file(temp_path: &Path, target_path: &Path) -> std::io::Result<()> {
    use std::iter::once;
    use std::os::windows::ffi::OsStrExt;

    unsafe extern "system" {
        fn MoveFileExW(from: *const u16, to: *const u16, flags: u32) -> i32;
    }

    const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x8;
    let from: Vec<u16> = temp_path.as_os_str().encode_wide().chain(once(0)).collect();
    let to: Vec<u16> = target_path.as_os_str().encode_wide().chain(once(0)).collect();

    let replaced = unsafe {
        MoveFileExW(
            from.as_ptr(),
            to.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if replaced == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn read_file_stably(path: &Path) -> std::io::Result<(String, std::fs::Metadata)> {
    for attempt in 0..3 {
        let before = std::fs::metadata(path)?;
        let content = std::fs::read_to_string(path)?;
        let after = std::fs::metadata(path)?;
        let content_again = std::fs::read_to_string(path)?;
        if before.len() == after.len()
            && content.as_bytes().len() as u64 == after.len()
            && content == content_again
        {
            return Ok((content, after));
        }
        if attempt < 2 {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::WouldBlock,
        "file changed while it was being read",
    ))
}

#[derive(Clone)]
struct LineDocument {
    lines: Vec<String>,
    eol: String,
    trailing_newline: bool,
}

fn detect_eol(content: &str) -> Result<String, String> {
    let bytes = content.as_bytes();
    let mut crlf = 0;
    let mut lf = 0;
    let mut lone_cr = 0;
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'\r' if bytes.get(index + 1) == Some(&b'\n') => {
                crlf += 1;
                index += 2;
            }
            b'\n' => {
                lf += 1;
                index += 1;
            }
            b'\r' => {
                lone_cr += 1;
                index += 1;
            }
            _ => index += 1,
        }
    }
    if lone_cr > 0 || (crlf > 0 && lf > 0) {
        return Err("mixed_line_endings".into());
    }
    if crlf > 0 {
        Ok("\r\n".into())
    } else {
        Ok("\n".into())
    }
}

fn parse_document(content: &str) -> Result<LineDocument, String> {
    let eol = detect_eol(content)?;
    let normalized = if eol == "\r\n" {
        content.replace("\r\n", "\n")
    } else {
        content.to_string()
    };
    let trailing_newline = normalized.ends_with('\n');
    let body = if trailing_newline {
        normalized.strip_suffix('\n').unwrap_or("")
    } else {
        normalized.as_str()
    };
    let lines = if body.is_empty() {
        if trailing_newline {
            vec![String::new()]
        } else {
            Vec::new()
        }
    } else {
        body.split('\n').map(str::to_string).collect()
    };
    Ok(LineDocument {
        lines,
        eol,
        trailing_newline,
    })
}

fn parse_operation_lines(text: &str) -> Result<(Vec<String>, bool), String> {
    let eol = detect_eol(text)?;
    let normalized = if eol == "\r\n" {
        text.replace("\r\n", "\n")
    } else {
        text.to_string()
    };
    let trailing = normalized.ends_with('\n');
    let body = if trailing {
        normalized.strip_suffix('\n').unwrap_or("")
    } else {
        normalized.as_str()
    };
    let lines = if body.is_empty() {
        if trailing {
            vec![String::new()]
        } else {
            Vec::new()
        }
    } else {
        body.split('\n').map(str::to_string).collect()
    };
    Ok((lines, trailing))
}

fn render_document(document: &LineDocument) -> String {
    if document.lines.is_empty() {
        return String::new();
    }
    let mut result = document.lines.join(&document.eol);
    if document.trailing_newline {
        result.push_str(&document.eol);
    }
    result
}

fn operation_range_error(index: usize, message: impl Into<String>) -> (usize, String) {
    (index, message.into())
}

fn apply_patch(content: &str, operations: &[PatchOp]) -> Result<String, (usize, String)> {
    let document = parse_document(content).map_err(|reason| (0, reason))?;
    let line_count = document.lines.len();

    struct ResolvedOp {
        start: usize,
        end: usize,
        kind: OpKind,
        op_index: usize,
    }
    enum OpKind {
        Replace(Vec<String>, bool),
        Insert(Vec<String>, bool),
        Delete,
        Append(Vec<String>, bool),
    }

    let mut resolved = Vec::with_capacity(operations.len());
    for (index, operation) in operations.iter().enumerate() {
        match operation {
            PatchOp::ReplaceLines {
                start_line,
                end_line,
                expected_hash,
                text,
            } => {
                if text.is_empty() {
                    return Err(operation_range_error(index, "empty_replacement"));
                }
                if *start_line == 0 || *end_line == 0 || start_line > end_line {
                    return Err(operation_range_error(index, "invalid_line_range"));
                }
                if *end_line > line_count {
                    return Err(operation_range_error(index, "line_out_of_bounds"));
                }
                if let Some(expected) = expected_hash {
                    let actual = lines_hash(&document.lines, *start_line, *end_line);
                    if expected != &actual {
                        return Err(operation_range_error(index, "anchor_changed"));
                    }
                }
                let (lines, trailing) = parse_operation_lines(text)
                    .map_err(|reason| operation_range_error(index, reason))?;
                resolved.push(ResolvedOp {
                    start: *start_line - 1,
                    end: *end_line - 1,
                    kind: OpKind::Replace(lines, trailing),
                    op_index: index,
                });
            }
            PatchOp::InsertBefore {
                line,
                anchor_hash,
                text,
            }
            | PatchOp::InsertAfter {
                line,
                anchor_hash,
                text,
            } => {
                if text.is_empty() {
                    return Err(operation_range_error(index, "empty_insertion"));
                }
                if *line == 0 || *line > line_count {
                    return Err(operation_range_error(index, "line_out_of_bounds"));
                }
                if let Some(expected) = anchor_hash {
                    let actual = line_hash(&document.lines[*line - 1]);
                    if expected != &actual {
                        return Err(operation_range_error(index, "anchor_changed"));
                    }
                }
                let (lines, trailing) = parse_operation_lines(text)
                    .map_err(|reason| operation_range_error(index, reason))?;
                let is_before = matches!(operation, PatchOp::InsertBefore { .. });
                let point = if is_before { *line - 1 } else { *line };
                resolved.push(ResolvedOp {
                    start: point,
                    end: point,
                    kind: OpKind::Insert(lines, trailing),
                    op_index: index,
                });
            }
            PatchOp::DeleteLines {
                start_line,
                end_line,
                expected_hash,
            } => {
                if *start_line == 0 || *end_line == 0 || start_line > end_line {
                    return Err(operation_range_error(index, "invalid_line_range"));
                }
                if *end_line > line_count {
                    return Err(operation_range_error(index, "line_out_of_bounds"));
                }
                if let Some(expected) = expected_hash {
                    let actual = lines_hash(&document.lines, *start_line, *end_line);
                    if expected != &actual {
                        return Err(operation_range_error(index, "anchor_changed"));
                    }
                }
                resolved.push(ResolvedOp {
                    start: *start_line - 1,
                    end: *end_line - 1,
                    kind: OpKind::Delete,
                    op_index: index,
                });
            }
            PatchOp::Append {
                expected_tail_hash,
                text,
            } => {
                if text.is_empty() {
                    return Err(operation_range_error(index, "empty_append"));
                }
                if let Some(expected) = expected_tail_hash {
                    let Some(last_line) = document.lines.last() else {
                        return Err(operation_range_error(index, "empty_file_tail"));
                    };
                    if expected != &line_hash(last_line) {
                        return Err(operation_range_error(index, "anchor_changed"));
                    }
                }
                let (lines, trailing) = parse_operation_lines(text)
                    .map_err(|reason| operation_range_error(index, reason))?;
                resolved.push(ResolvedOp {
                    start: line_count,
                    end: line_count,
                    kind: OpKind::Append(lines, trailing),
                    op_index: index,
                });
            }
        }
    }

    let ranges: Vec<(usize, usize, usize)> = resolved
        .iter()
        .filter_map(|operation| match operation.kind {
            OpKind::Replace(_, _) | OpKind::Delete => {
                Some((operation.start, operation.end, operation.op_index))
            }
            _ => None,
        })
        .collect();

    for (left_index, left) in ranges.iter().enumerate() {
        for right in ranges.iter().skip(left_index + 1) {
            if left.0 <= right.1 && right.0 <= left.1 {
                return Err(operation_range_error(right.2, "overlapping_ranges"));
            }
        }
    }

    let mut insertion_points: HashMap<usize, usize> = HashMap::new();
    for operation in &resolved {
        if matches!(operation.kind, OpKind::Insert(_, _) | OpKind::Append(_, _)) {
            if insertion_points.insert(operation.start, operation.op_index).is_some() {
                return Err(operation_range_error(operation.op_index, "duplicate_insertion_point"));
            }
            for (start, end, range_index) in &ranges {
                if operation.start >= *start && operation.start <= end + 1 {
                    return Err(operation_range_error(
                        operation.op_index,
                        format!("insertion_overlaps_range_{range_index}"),
                    ));
                }
            }
        }
    }

    resolved.sort_by_key(|operation| Reverse(operation.start));
    let mut result = document.clone();
    for operation in resolved {
        match operation.kind {
            OpKind::Replace(lines, trailing) => {
                let old_end = operation.end;
                result.lines.splice(operation.start..=old_end, lines);
                if old_end + 1 == line_count {
                    result.trailing_newline = trailing;
                }
            }
            OpKind::Insert(lines, trailing) => {
                let at_end = operation.start == result.lines.len();
                result.lines.splice(operation.start..operation.start, lines);
                if at_end && trailing {
                    result.trailing_newline = true;
                }
            }
            OpKind::Delete => {
                let at_end = operation.end + 1 == result.lines.len();
                result.lines.splice(operation.start..=operation.end, std::iter::empty());
                if result.lines.is_empty() {
                    result.trailing_newline = false;
                } else if at_end {
                    result.trailing_newline = document.trailing_newline;
                }
            }
            OpKind::Append(lines, trailing) => {
                result.lines.extend(lines);
                result.trailing_newline = trailing;
            }
        }
    }
    Ok(render_document(&result))
}

fn fingerprint<T: Serialize>(method: &str, body: &T) -> String {
    let mut hasher = Sha256::new();
    hasher.update(method.as_bytes());
    hasher.update([0]);
    hasher.update(serde_json::to_vec(body).expect("serializable request"));
    format!("sha256:{:x}", hasher.finalize())
}

pub fn api_routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/tree", get(get_tree))
        .route("/search", get(search_files))
        .route("/markdown-paths", get(get_markdown_paths))
        .route("/file", get(get_file).put(put_file).patch(patch_file))
        .route("/config", get(get_config).put(put_config))
}

pub fn raw_routes() -> Router<Arc<AppState>> {
    Router::new().route("/raw/{*path}", get(get_raw))
}

async fn get_tree(
    State(state): State<Arc<AppState>>,
    Query(query): Query<TreeQuery>,
) -> Result<Json<Vec<FileNode>>, StatusCode> {
    let directory = match query.path.as_deref() {
        Some(path) if !path.is_empty() => vault_relative_path(&state.vault_path, path)?,
        _ => state.vault_path.clone(),
    };
    if !directory.is_dir() {
        return Err(StatusCode::NOT_FOUND);
    }
    let nodes = scan_dir(&state.vault_path, &directory).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(nodes))
}

async fn search_files(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<FileNode>>, StatusCode> {
    let query = query.query.trim().to_lowercase();
    if query.is_empty() {
        return Ok(Json(Vec::new()));
    }
    let mut results = Vec::new();
    collect_search_results(&state.vault_path, &state.vault_path, &query, &mut results)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    results.sort_by(|a, b| a.path.to_lowercase().cmp(&b.path.to_lowercase()));
    Ok(Json(results))
}

async fn get_markdown_paths(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<String>>, StatusCode> {
    let mut paths = Vec::new();
    collect_markdown_paths(&state.vault_path, &state.vault_path, &mut paths)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    paths.sort_unstable();
    Ok(Json(paths))
}

fn collect_markdown_paths(base: &Path, dir: &Path, paths: &mut Vec<String>) -> Result<(), std::io::Error> {
    for entry in std::fs::read_dir(dir)?.filter_map(Result::ok) {
        let metadata = entry.metadata()?;
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') && (metadata.is_dir() || name.contains(".catdo-")) {
            continue;
        }
        if metadata.is_dir() {
            if should_skip(&name) {
                continue;
            }
            collect_markdown_paths(base, &entry.path(), paths)?;
        } else if name.to_lowercase().ends_with(".md") {
            paths.push(entry.path().strip_prefix(base).unwrap().to_string_lossy().replace('\\', "/"));
        }
    }
    Ok(())
}

fn scan_dir(base: &Path, dir: &Path) -> Result<Vec<FileNode>, std::io::Error> {
    let mut entries = Vec::new();
    let mut read_dir: Vec<_> = std::fs::read_dir(dir)?.filter_map(Result::ok).collect();
    read_dir.sort_by_key(|entry| entry.file_name());
    for entry in read_dir {
        let metadata = entry.metadata()?;
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') && name.contains(".catdo-") {
            continue;
        }
        if name.starts_with('.') && metadata.is_dir() {
            continue;
        }
        if metadata.is_dir() && should_skip(&name) {
            continue;
        }
        let relative = entry.path().strip_prefix(base).unwrap().to_string_lossy().replace('\\', "/");
        entries.push(FileNode {
            name,
            path: relative,
            is_dir: metadata.is_dir(),
            size: if metadata.is_dir() { 0 } else { metadata.len() },
            modified: metadata.modified().ok().and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |duration| duration.as_secs()),
            children: None,
        });
    }
    entries.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });
    Ok(entries)
}

fn collect_search_results(base: &Path, dir: &Path, query: &str, results: &mut Vec<FileNode>) -> Result<(), std::io::Error> {
    for entry in std::fs::read_dir(dir)?.filter_map(Result::ok) {
        let metadata = entry.metadata()?;
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') && (metadata.is_dir() || name.contains(".catdo-")) {
            continue;
        }
        if metadata.is_dir() {
            if should_skip(&name) {
                continue;
            }
            collect_search_results(base, &entry.path(), query, results)?;
            continue;
        }
        let relative = entry.path().strip_prefix(base).unwrap().to_string_lossy().replace('\\', "/");
        if relative.to_lowercase().contains(query) {
            results.push(FileNode {
                name,
                path: relative,
                is_dir: false,
                size: metadata.len(),
                modified: metadata.modified().ok().and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |duration| duration.as_secs()),
                children: None,
            });
        }
    }
    Ok(())
}

async fn get_file(
    State(state): State<Arc<AppState>>,
    Query(query): Query<FileQuery>,
) -> Result<Json<FileContent>, StatusCode> {
    let file_path = vault_relative_path(&state.vault_path, &query.path)?;
    let lock = state.write_locks.lock_for(&file_path);
    let _guard = lock.lock();
    if !file_path.is_file() {
        return Err(StatusCode::NOT_FOUND);
    }
    let (content, metadata) = read_file_stably(&file_path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound { StatusCode::NOT_FOUND } else { StatusCode::INTERNAL_SERVER_ERROR }
    })?;
    let modified = metadata.modified().ok().and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |duration| duration.as_secs());
    Ok(Json(FileContent { path: query.path, modified, version: content_version(&content), content }))
}

async fn put_file(
    State(state): State<Arc<AppState>>,
    Json(body): Json<FileSave>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let file_path = vault_relative_path(&state.vault_path, &body.path).map_err(|status| json_error(status, "bad_path"))?;
    let request_fingerprint = fingerprint("PUT", &body);
    let request_lock = body.request_id.as_deref().map(|id| state.idempotency.lock_for(id));
    let _request_guard = request_lock.as_ref().map(|lock| lock.lock());
    let lock = state.write_locks.lock_for(&file_path);
    let _guard = lock.lock();

    if let Some(request_id) = body.request_id.as_deref() {
        match state.idempotency.check(request_id, &request_fingerprint) {
            Ok(Some(response)) => return Ok(Json(response)),
            Ok(None) => {}
            Err(()) => return Err(json_error(StatusCode::CONFLICT, "idempotency_key_reused")),
        }
    }

    if file_path.exists() {
        if !file_path.is_file() {
            return Err(json_error(StatusCode::CONFLICT, "not_a_file"));
        }
        let (current_content, _) = read_file_stably(&file_path).map_err(|_| json_error(StatusCode::CONFLICT, "read_failed"))?;
        let current_version = content_version(&current_content);
        let Some(expected) = body.expected_version.as_deref() else {
            return Err((StatusCode::CONFLICT, Json(serde_json::json!({ "error": "version_required", "current_version": current_version }))));
        };
        if expected != current_version {
            return Err((StatusCode::CONFLICT, Json(serde_json::json!({ "error": "version_mismatch", "current_version": current_version }))));
        }
    } else if body.expected_version.is_some() {
        return Err(json_error(StatusCode::CONFLICT, "file_not_found_with_version"));
    }

    if let Some(parent) = file_path.parent() {
        std::fs::create_dir_all(parent).map_err(|_| json_error(StatusCode::INTERNAL_SERVER_ERROR, "mkdir_failed"))?;
    }
    // Revalidate after creating missing parents to close the symlink-swap window.
    vault_relative_path(&state.vault_path, &body.path)
        .map_err(|status| json_error(status, "bad_path"))?;
    write_atomically(&file_path, body.content.as_bytes()).map_err(|_| json_error(StatusCode::INTERNAL_SERVER_ERROR, "write_failed"))?;
    let response = serde_json::json!({ "ok": true, "path": body.path, "version": content_version(&body.content) });
    if let Some(request_id) = body.request_id {
        state.idempotency.record(request_id, request_fingerprint, response.clone());
    }
    Ok(Json(response))
}

async fn patch_file(
    State(state): State<Arc<AppState>>,
    Json(body): Json<FilePatch>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let file_path = vault_relative_path(&state.vault_path, &body.path).map_err(|status| json_error(status, "bad_path"))?;
    let request_fingerprint = fingerprint("PATCH", &body);
    let request_lock = body.request_id.as_deref().map(|id| state.idempotency.lock_for(id));
    let _request_guard = request_lock.as_ref().map(|lock| lock.lock());
    let lock = state.write_locks.lock_for(&file_path);
    let _guard = lock.lock();

    if let Some(request_id) = body.request_id.as_deref() {
        match state.idempotency.check(request_id, &request_fingerprint) {
            Ok(Some(response)) => return Ok(Json(response)),
            Ok(None) => {}
            Err(()) => return Err(json_error(StatusCode::CONFLICT, "idempotency_key_reused")),
        }
    }

    if !file_path.is_file() {
        return Err(json_error(StatusCode::NOT_FOUND, "file_not_found"));
    }
    let (current_content, _) = read_file_stably(&file_path).map_err(|_| json_error(StatusCode::INTERNAL_SERVER_ERROR, "read_failed"))?;
    let current_version = content_version(&current_content);
    if body.base_version != current_version {
        return Err((StatusCode::CONFLICT, Json(serde_json::json!({ "error": "stale_snapshot", "current_version": current_version }))));
    }

    if body.operations.is_empty() {
        let response = serde_json::json!({ "ok": true, "path": body.path, "version": current_version, "applied_operations": 0 });
        if let Some(request_id) = body.request_id {
            state.idempotency.record(request_id, request_fingerprint, response.clone());
        }
        return Ok(Json(response));
    }

    let new_content = apply_patch(&current_content, &body.operations).map_err(|(index, reason)| {
        (StatusCode::UNPROCESSABLE_ENTITY, Json(serde_json::json!({ "error": "patch_failed", "current_version": current_version, "failed_operation": { "index": index, "reason": reason } })))
    })?;
    // Revalidate immediately before replacement; the version check and this
    // validation both occur while the per-path lock is held.
    vault_relative_path(&state.vault_path, &body.path)
        .map_err(|status| json_error(status, "bad_path"))?;
    let version = content_version(&new_content);
    let response = serde_json::json!({ "ok": true, "path": body.path, "version": version, "applied_operations": body.operations.len() });
    if new_content != current_content {
        write_atomically(&file_path, new_content.as_bytes()).map_err(|_| json_error(StatusCode::INTERNAL_SERVER_ERROR, "write_failed"))?;
    }
    if let Some(request_id) = body.request_id {
        state.idempotency.record(request_id, request_fingerprint, response.clone());
    }
    Ok(Json(response))
}

async fn get_config(State(state): State<Arc<AppState>>) -> Result<Json<config::VaultConfig>, StatusCode> {
    Ok(Json(config::load_config(&state.vault_path)))
}

async fn put_config(State(state): State<Arc<AppState>>, Json(cfg): Json<config::VaultConfig>) -> Result<Json<Value>, StatusCode> {
    config::save_config(&state.vault_path, &cfg);
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn get_raw(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(rel_path): axum::extract::Path<String>,
) -> Result<axum::response::Response, StatusCode> {
    let file_path = vault_relative_path(&state.vault_path, &rel_path)?;
    let lock = state.write_locks.lock_for(&file_path);
    let _guard = lock.lock();
    if !file_path.is_file() {
        return Err(StatusCode::NOT_FOUND);
    }
    let bytes = std::fs::read(&file_path).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mime = mime_guess::from_path(&file_path).first_or_octet_stream().to_string();
    Ok(axum::response::Response::builder()
        .header("Content-Type", mime)
        .header("Cache-Control", "public, max-age=3600")
        .body(axum::body::Body::from(bytes))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?)
}
