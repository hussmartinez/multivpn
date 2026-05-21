use crate::types::ProviderKind;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub general: GeneralConfig,
    #[serde(default)]
    pub autoconnect: AutoconnectConfig,
    #[serde(default)]
    pub providers: HashMap<String, ProviderConfig>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct GeneralConfig {
    #[serde(default)]
    pub kill_switch: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AutoconnectConfig {
    #[serde(default)]
    pub connections: Vec<AutoconnectEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AutoconnectEntry {
    pub provider: ProviderKind,
    pub id: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub config_dir: Option<String>,
    #[serde(flatten)]
    pub extra: HashMap<String, toml::Value>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            general: GeneralConfig::default(),
            autoconnect: AutoconnectConfig::default(),
            providers: HashMap::new(),
        }
    }
}

impl Config {
    pub fn config_dir() -> PathBuf {
        dirs_or_default()
    }

    pub fn config_path() -> PathBuf {
        Self::config_dir().join("config.toml")
    }

    pub fn load() -> Result<Self> {
        let path = Self::config_path();
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let config: Config = toml::from_str(&content)
            .with_context(|| format!("failed to parse {}", path.display()))?;
        Ok(config)
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::config_path();
        let dir = path.parent().unwrap();
        std::fs::create_dir_all(dir)
            .with_context(|| format!("failed to create {}", dir.display()))?;
        let content = toml::to_string_pretty(self)?;
        std::fs::write(&path, content)
            .with_context(|| format!("failed to write {}", path.display()))?;
        Ok(())
    }

    pub fn load_from(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let config: Config = toml::from_str(&content)
            .with_context(|| format!("failed to parse {}", path.display()))?;
        Ok(config)
    }

    pub fn save_to(&self, path: &Path) -> Result<()> {
        let dir = path.parent().unwrap();
        std::fs::create_dir_all(dir)
            .with_context(|| format!("failed to create {}", dir.display()))?;
        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)
            .with_context(|| format!("failed to write {}", path.display()))?;
        Ok(())
    }

    pub fn provider_config_dir(&self, kind: ProviderKind) -> Option<&Path> {
        self.providers
            .get(kind.as_str())
            .and_then(|p| p.config_dir.as_deref())
            .map(Path::new)
    }
}

pub fn parse_from_str(content: &str) -> Result<Config> {
    Ok(toml::from_str(content)?)
}

fn dirs_or_default() -> PathBuf {
    std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
            PathBuf::from(home).join(".config")
        })
        .join("multivpn")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config() {
        let cfg = Config::default();
        assert!(!cfg.general.kill_switch);
        assert!(cfg.autoconnect.connections.is_empty());
        assert!(cfg.providers.is_empty());
    }

    #[test]
    fn parse_empty_string() {
        let cfg: Config = toml::from_str("").unwrap();
        assert!(!cfg.general.kill_switch);
        assert!(cfg.autoconnect.connections.is_empty());
    }

    #[test]
    fn parse_full_config() {
        let toml_str = r#"
[general]
kill_switch = true

[[autoconnect.connections]]
provider = "wireguard"
id = "wg0"

[[autoconnect.connections]]
provider = "tailscale"
id = "default"

[providers.wireguard]
config_dir = "/etc/wireguard"

[providers.openvpn]
config_dir = "/etc/openvpn"
"#;
        let cfg = parse_from_str(toml_str).unwrap();
        assert!(cfg.general.kill_switch);
        assert_eq!(cfg.autoconnect.connections.len(), 2);
        assert_eq!(
            cfg.autoconnect.connections[0].provider,
            ProviderKind::WireGuard
        );
        assert_eq!(cfg.autoconnect.connections[0].id, "wg0");
        assert_eq!(
            cfg.autoconnect.connections[1].provider,
            ProviderKind::Tailscale
        );
        assert_eq!(
            cfg.provider_config_dir(ProviderKind::WireGuard),
            Some(Path::new("/etc/wireguard"))
        );
        assert_eq!(
            cfg.provider_config_dir(ProviderKind::OpenVpn),
            Some(Path::new("/etc/openvpn"))
        );
        assert_eq!(cfg.provider_config_dir(ProviderKind::Tailscale), None);
    }

    #[test]
    fn parse_general_only() {
        let cfg: Config = toml::from_str("[general]\nkill_switch = true\n").unwrap();
        assert!(cfg.general.kill_switch);
        assert!(cfg.autoconnect.connections.is_empty());
    }

    #[test]
    fn parse_provider_with_extra_fields() {
        let toml_str = r#"
[providers.wireguard]
config_dir = "/etc/wireguard"
custom_flag = true
"#;
        let cfg = parse_from_str(toml_str).unwrap();
        let wg = cfg.providers.get("wireguard").unwrap();
        assert_eq!(wg.config_dir.as_deref(), Some("/etc/wireguard"));
        assert!(wg.extra.contains_key("custom_flag"));
    }

    #[test]
    fn config_roundtrip_toml() {
        let mut cfg = Config::default();
        cfg.general.kill_switch = true;
        cfg.autoconnect.connections.push(AutoconnectEntry {
            provider: ProviderKind::WireGuard,
            id: "wg0".into(),
        });
        let serialized = toml::to_string_pretty(&cfg).unwrap();
        let back = parse_from_str(&serialized).unwrap();
        assert!(back.general.kill_switch);
        assert_eq!(back.autoconnect.connections.len(), 1);
        assert_eq!(back.autoconnect.connections[0].id, "wg0");
    }

    #[test]
    fn save_to_and_load_from() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("multivpn/config.toml");

        let mut cfg = Config::default();
        cfg.general.kill_switch = true;
        cfg.autoconnect.connections.push(AutoconnectEntry {
            provider: ProviderKind::WireGuard,
            id: "wg0".into(),
        });
        cfg.save_to(&path).unwrap();

        let loaded = Config::load_from(&path).unwrap();
        assert!(loaded.general.kill_switch);
        assert_eq!(loaded.autoconnect.connections.len(), 1);
        assert_eq!(loaded.autoconnect.connections[0].id, "wg0");
    }

    #[test]
    fn load_from_missing_file_returns_default() {
        let path = Path::new("/tmp/mvpn_test_nonexistent_12345/config.toml");
        let cfg = Config::load_from(path).unwrap();
        assert!(!cfg.general.kill_switch);
        assert!(cfg.autoconnect.connections.is_empty());
    }
}
