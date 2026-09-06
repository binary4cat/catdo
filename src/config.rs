use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct VaultConfig {
    pub version: u32,
    pub assets_dir_name: String,
    pub theme: String,
    pub auto_save_interval_ms: u64,
    #[serde(default)]
    pub wiki_links: WikiLinksConfig,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct WikiLinksConfig {
    pub auto_create: bool,
}

impl Default for WikiLinksConfig {
    fn default() -> Self {
        Self { auto_create: true }
    }
}

impl Default for VaultConfig {
    fn default() -> Self {
        Self {
            version: 1,
            assets_dir_name: "_assets".to_string(),
            theme: "system".to_string(),
            auto_save_interval_ms: 1000,
            wiki_links: WikiLinksConfig { auto_create: true },
        }
    }
}

pub fn init_config(vault_path: &Path) -> VaultConfig {
    let config_dir = vault_path.join(".catdo");
    let config_file = config_dir.join("config.json");

    if !config_dir.exists() {
        fs::create_dir_all(&config_dir).expect("Failed to create .catdo directory");
    }

    if config_file.exists() {
        let content = fs::read_to_string(&config_file).expect("Failed to read config.json");
        serde_json::from_str(&content).unwrap_or_else(|_| {
            let config = VaultConfig::default();
            write_config(&config_file, &config);
            config
        })
    } else {
        let config = VaultConfig::default();
        write_config(&config_file, &config);
        config
    }
}

pub fn load_config(vault_path: &Path) -> VaultConfig {
    let config_file = vault_path.join(".catdo/config.json");
    if config_file.exists() {
        let content = fs::read_to_string(&config_file).unwrap_or_default();
        serde_json::from_str(&content).unwrap_or_default()
    } else {
        VaultConfig::default()
    }
}

pub fn save_config(vault_path: &Path, config: &VaultConfig) {
    let config_file = vault_path.join(".catdo/config.json");
    write_config(&config_file, config);
}

fn write_config(path: &Path, config: &VaultConfig) {
    let json = serde_json::to_string_pretty(config).expect("Failed to serialize config");
    fs::write(path, json).expect("Failed to write config.json");
}
