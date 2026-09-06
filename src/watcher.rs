use std::path::{Path, PathBuf};
use std::sync::Arc;

use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::broadcast;

const HIDDEN_DIRS: &[&str] = &[".catdo", ".git", ".obsidian"];

#[derive(Clone)]
pub struct WatcherState {
    tx: Arc<broadcast::Sender<String>>,
}

impl WatcherState {
    pub fn subscribe(&self) -> broadcast::Receiver<String> {
        self.tx.subscribe()
    }
}

fn is_hidden_path(path: &Path, vault_root: &Path) -> bool {
    if let Some(name) = path.file_name().and_then(|value| value.to_str()) {
        if name.starts_with('.') && name.contains(".catdo-") {
            return true;
        }
    }

    if let Ok(rel) = path.strip_prefix(vault_root) {
        for component in rel.components() {
            let s = component.as_os_str().to_string_lossy();
            if HIDDEN_DIRS.iter().any(|&h| s == h) {
                return true;
            }
        }
    }
    false
}

fn event_kind_to_string(kind: &notify::EventKind) -> Option<&'static str> {
    use notify::EventKind::*;
    match kind {
        Create(_) => Some("create"),
        Modify(_) => Some("change"),
        Remove(_) => Some("remove"),
        _ => None,
    }
}

pub fn start_watcher(vault_path: &Path) -> (WatcherState, RecommendedWatcher) {
    let (tx, _) = broadcast::channel::<String>(256);
    let tx = Arc::new(tx);
    let state = WatcherState { tx: tx.clone() };

    let vault_root: PathBuf = vault_path.to_path_buf();

    let mut watcher = RecommendedWatcher::new(
        move |result: Result<Event, notify::Error>| {
            if let Ok(event) = result {
                if let Some(event_type) = event_kind_to_string(&event.kind) {
                    for path in &event.paths {
                        if is_hidden_path(path, &vault_root) {
                            continue;
                        }
                        if let Ok(rel) = path.strip_prefix(&vault_root) {
                            let rel_str = rel.to_string_lossy().replace('\\', "/");
                            let msg = serde_json::json!({
                                "event": event_type,
                                "path": rel_str,
                            });
                            let _ = tx.send(msg.to_string());
                        }
                    }
                }
            }
        },
        Config::default(),
    )
    .expect("Failed to create file watcher");

    watcher
        .watch(vault_path, RecursiveMode::Recursive)
        .expect("Failed to start watching vault directory");

    (state, watcher)
}
