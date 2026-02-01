//! Parse and validate config.toml (run config + optional security + optional desktop).

use serde::Deserialize;
use std::path::Path;

/// Root config.toml structure.
#[derive(Debug, Deserialize)]
pub struct Config {
    /// Required: app name (for menu + profile)
    pub name: String,
    /// Required: path to executable relative to bundle root
    pub executable: String,
    /// Optional: args to pass to executable
    #[serde(default)]
    pub args: Vec<String>,
    /// Optional: env vars (key=value)
    #[serde(default)]
    pub env: Vec<String>,
    /// Optional: working directory (relative to bundle root)
    pub working_dir: Option<String>,
    /// Optional: desktop metadata for generated .desktop
    pub icon: Option<String>,
    pub comment: Option<String>,
    pub categories: Option<Vec<String>>,
    /// When true, add Terminal=true so the app is run in a terminal (for CLI apps with no UI).
    #[serde(default)]
    pub terminal: bool,
    /// Optional: security section for AppArmor
    #[serde(default)]
    pub security: Option<Security>,
}

/// Security requirements for AppArmor profile generation.
#[derive(Debug, Deserialize)]
pub struct Security {
    /// When false, run without AppArmor (no confinement). Use for Electron/Chromium apps that
    /// fail under confinement. Default true.
    #[serde(default = "default_confine")]
    pub confine: bool,
    #[serde(default)]
    pub read_paths: Vec<String>,
    #[serde(default)]
    pub write_paths: Vec<String>,
    #[serde(default)]
    pub network: bool,
    #[serde(default)]
    #[allow(dead_code)] // reserved for future AppArmor capability rules
    pub capabilities: Vec<String>,
}

impl Default for Security {
    fn default() -> Self {
        Self {
            confine: true,
            read_paths: Vec::new(),
            write_paths: Vec::new(),
            network: false,
            capabilities: Vec::new(),
        }
    }
}

fn default_confine() -> bool {
    true
}

/// Load and parse config.toml from a bundle root directory.
pub fn load(bundle_root: &Path) -> anyhow::Result<Config> {
    let path = bundle_root.join("config.toml");
    let s = std::fs::read_to_string(&path)
        .map_err(|e| anyhow::anyhow!("failed to read config.toml: {}", e))?;
    let config: Config = toml::from_str(&s).map_err(|e| anyhow::anyhow!("invalid config.toml: {}", e))?;
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_minimal_config() {
        let dir = tempfile::tempdir().unwrap();
        let config_toml = dir.path().join("config.toml");
        std::fs::write(
            &config_toml,
            r#"
name = "myapp"
executable = "bin/myapp"
"#,
        )
        .unwrap();
        let cfg = load(dir.path()).unwrap();
        assert_eq!(cfg.name, "myapp");
        assert_eq!(cfg.executable, "bin/myapp");
        assert!(cfg.args.is_empty());
        assert!(cfg.security.is_none());
    }

    #[test]
    fn load_config_with_optional_fields() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("config.toml"),
            r#"
name = "full"
executable = "bin/full"
args = ["--verbose"]
env = ["FOO=bar"]
working_dir = "data"
icon = "myapp"
comment = "A test app"
categories = ["Utility", "Development"]

[security]
read_paths = ["/tmp/read"]
write_paths = ["/tmp/write"]
network = true
"#,
        )
        .unwrap();
        let cfg = load(dir.path()).unwrap();
        assert_eq!(cfg.name, "full");
        assert_eq!(cfg.args, ["--verbose"]);
        assert_eq!(cfg.env, ["FOO=bar"]);
        assert_eq!(cfg.working_dir.as_deref(), Some("data"));
        assert_eq!(cfg.icon.as_deref(), Some("myapp"));
        let sec = cfg.security.as_ref().unwrap();
        assert_eq!(sec.read_paths, ["/tmp/read"]);
        assert_eq!(sec.write_paths, ["/tmp/write"]);
        assert!(sec.network);
    }

    #[test]
    fn load_missing_file_err() {
        let dir = tempfile::tempdir().unwrap();
        let err = load(dir.path()).unwrap_err();
        assert!(err.to_string().contains("config.toml"));
    }

    #[test]
    fn load_invalid_toml_err() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("config.toml"), "name = invalid toml [[[").unwrap();
        let err = load(dir.path()).unwrap_err();
        assert!(err.to_string().to_lowercase().contains("invalid"));
    }
}
