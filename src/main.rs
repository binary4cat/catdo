use std::path::PathBuf;

use clap::Parser;
use tokio::net::TcpListener;

mod config;
mod server;
mod watcher;

#[derive(Parser, Debug)]
#[command(name = "catdo", about = "Local knowledge base engine")]
struct Cli {
    /// Path to the vault directory (defaults to current directory)
    #[arg(default_value = ".")]
    vault_path: PathBuf,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let vault_path = std::fs::canonicalize(&cli.vault_path)
        .unwrap_or_else(|_| panic!("Cannot resolve vault path: {:?}", cli.vault_path));

    if !vault_path.is_dir() {
        eprintln!("Error: {:?} is not a directory", vault_path);
        std::process::exit(1);
    }

    // Initialize config
    let _config = config::init_config(&vault_path);
    println!("[catdo] Vault: {}", vault_path.display());

    // Set up file watcher
    let (watcher_state, _watcher_handle) = watcher::start_watcher(&vault_path);

    // Build router
    let app = server::build_router(vault_path.clone(), watcher_state);

    // Bind to a random available port
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    println!("[catdo] Listening on http://{}", addr);

    // Open browser
    let url = format!("http://{}", addr);
    if let Err(e) = open::that(&url) {
        eprintln!("[catdo] Failed to open browser: {}", e);
    }

    // Run server
    axum::serve(listener, app).await.unwrap();
}
