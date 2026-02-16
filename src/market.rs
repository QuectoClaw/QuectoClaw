// QuectoClaw — Plugin Marketplace
//
// Handles remote discovery and local installation of QuectoClaw plugins.

use serde::{Deserialize, Serialize};
use std::path::Path;

/// Metadata for a plugin in the remote registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketPlugin {
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    pub download_url: String,
    pub r#type: PluginType,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PluginType {
    Shell,
    Wasm,
}

/// The main plugin registry structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginRegistry {
    pub plugins: Vec<MarketPlugin>,
}

pub struct PluginMarket {
    registry_url: String,
}

impl PluginMarket {
    pub fn new(registry_url: String) -> Self {
        Self { registry_url }
    }

    /// Fetch the list of available plugins from the remote registry.
    pub async fn fetch_registry(&self) -> anyhow::Result<PluginRegistry> {
        let response = reqwest::get(&self.registry_url).await?;
        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "Failed to fetch registry: {}",
                response.status()
            ));
        }
        let registry = response.json::<PluginRegistry>().await?;
        Ok(registry)
    }

    /// Install a plugin from the marketplace.
    pub async fn install_plugin(
        &self,
        plugin: &MarketPlugin,
        plugins_dir: &Path,
    ) -> anyhow::Result<()> {
        let target_dir = plugins_dir.join(&plugin.name);
        std::fs::create_dir_all(&target_dir)?;

        let response = reqwest::get(&plugin.download_url).await?;
        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "Failed to download plugin: {}",
                response.status()
            ));
        }

        let bytes = response.bytes().await?;

        match plugin.r#type {
            PluginType::Shell => {
                // For shell plugins, we expect a single JSON file for now
                let file_path = target_dir.join(format!("{}.json", plugin.name));
                std::fs::write(file_path, bytes)?;
            }
            PluginType::Wasm => {
                // For WASM plugins, we expect a zip or individual files
                // For simplicity in this initial version, we assume a single .wasm file
                // if it's just bytes, or we'd need zip extraction logic.
                // Let's assume the download_url points to a .wasm file if it ends in .wasm.
                if plugin.download_url.ends_with(".wasm") {
                    let file_path = target_dir.join(format!("{}.wasm", plugin.name));
                    std::fs::write(file_path, bytes)?;

                    // Create a basic manifest.json if it doesn't exist
                    let manifest_path = target_dir.join("manifest.json");
                    if !manifest_path.exists() {
                        let manifest = serde_json::json!({
                            "name": plugin.name,
                            "description": plugin.description,
                            "wasm_file": format!("{}.wasm", plugin.name),
                            "parameters": [],
                            "fuel": 1000000
                        });
                        std::fs::write(manifest_path, serde_json::to_string_pretty(&manifest)?)?;
                    }
                } else {
                    return Err(anyhow::anyhow!(
                        "Unsupported WASM package format (expected .wasm)"
                    ));
                }
            }
        }

        Ok(())
    }

    /// List locally installed plugins.
    pub fn list_installed(plugins_dir: &Path) -> anyhow::Result<Vec<String>> {
        let mut installed = Vec::new();
        if !plugins_dir.exists() {
            return Ok(installed);
        }

        for entry in std::fs::read_dir(plugins_dir)? {
            let entry = entry?;
            if entry.path().is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    installed.push(name.to_string());
                }
            }
        }
        Ok(installed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_list_installed_empty_dir() {
        let tmp = TempDir::new().unwrap();
        let result = PluginMarket::list_installed(tmp.path()).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_list_installed_nonexistent_dir() {
        let result = PluginMarket::list_installed(Path::new("/nonexistent/plugins")).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_list_installed_with_plugins() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir(tmp.path().join("my-plugin")).unwrap();
        std::fs::create_dir(tmp.path().join("another-plugin")).unwrap();
        // Files should be ignored — only directories count
        std::fs::write(tmp.path().join("not-a-plugin.txt"), "data").unwrap();

        let mut result = PluginMarket::list_installed(tmp.path()).unwrap();
        result.sort();
        assert_eq!(result, vec!["another-plugin", "my-plugin"]);
    }

    #[test]
    fn test_market_plugin_serialization() {
        let plugin = MarketPlugin {
            name: "test".into(),
            version: "1.0.0".into(),
            description: "A test plugin".into(),
            author: "tester".into(),
            download_url: "https://example.com/test.json".into(),
            r#type: PluginType::Shell,
        };
        let json = serde_json::to_string(&plugin).unwrap();
        let parsed: MarketPlugin = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "test");
        assert_eq!(parsed.r#type, PluginType::Shell);
    }

    #[test]
    fn test_plugin_registry_deserialization() {
        let json = r#"{
            "plugins": [
                {
                    "name": "formatter",
                    "version": "0.2.0",
                    "description": "Code formatter",
                    "author": "dev",
                    "download_url": "https://example.com/formatter.json",
                    "type": "shell"
                },
                {
                    "name": "wasm-calc",
                    "version": "1.0.0",
                    "description": "Calculator",
                    "author": "dev",
                    "download_url": "https://example.com/calc.wasm",
                    "type": "wasm"
                }
            ]
        }"#;
        let registry: PluginRegistry = serde_json::from_str(json).unwrap();
        assert_eq!(registry.plugins.len(), 2);
        assert_eq!(registry.plugins[0].r#type, PluginType::Shell);
        assert_eq!(registry.plugins[1].r#type, PluginType::Wasm);
    }

    #[test]
    fn test_plugin_type_variants() {
        assert_ne!(PluginType::Shell, PluginType::Wasm);

        let shell_json = serde_json::to_string(&PluginType::Shell).unwrap();
        assert_eq!(shell_json, "\"shell\"");

        let wasm_json = serde_json::to_string(&PluginType::Wasm).unwrap();
        assert_eq!(wasm_json, "\"wasm\"");
    }
}
