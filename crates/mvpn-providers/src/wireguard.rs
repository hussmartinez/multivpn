use crate::command::{self, command_exists, run, run_with_stdin};
use anyhow::{Context, Result, anyhow, bail};
use mvpn_core::provider::VpnProvider;
use mvpn_core::types::*;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

pub struct WireGuardProvider {
    config_dir: PathBuf,
}

impl WireGuardProvider {
    pub fn new() -> Self {
        Self {
            config_dir: PathBuf::from("/etc/wireguard"),
        }
    }

    fn config_path(&self, name: &str) -> PathBuf {
        self.config_dir.join(format!("{name}.conf"))
    }

    fn validate_name(name: &str) -> Result<()> {
        if name.is_empty() {
            bail!("interface name is required");
        }
        if !name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' || ch == '.')
        {
            bail!("interface name may only contain letters, numbers, '.', '_' and '-'");
        }
        Ok(())
    }

    fn active_interfaces(&self) -> BTreeSet<String> {
        run("wg", &["show", "interfaces"], true)
            .or_else(|_| run("wg", &["show", "interfaces"], false))
            .map(|output| output.split_whitespace().map(ToOwned::to_owned).collect())
            .unwrap_or_default()
    }

    fn is_enabled(&self, name: &str) -> bool {
        if !command_exists("systemctl") {
            return false;
        }
        std::process::Command::new("sudo")
            .arg("-n")
            .arg("systemctl")
            .arg("is-enabled")
            .arg(format!("wg-quick@{name}.service"))
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    fn read_config_files(&self) -> Result<Vec<(String, PathBuf)>> {
        let dir = &self.config_dir;
        let entries = match std::fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(_) => return Ok(Vec::new()),
        };

        let mut results = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("conf") {
                if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                    results.push((name.to_string(), path));
                }
            }
        }
        results.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(results)
    }

    fn generate_private_key(&self) -> Result<String> {
        run("wg", &["genkey"], false)
            .or_else(|_| run("wg", &["genkey"], true))
            .context(
                "failed to generate private key; install wireguard-tools or provide one explicitly",
            )
    }

    fn build_config_text(&self, config: &CreateRequest) -> Result<String> {
        let name = &config.name;
        Self::validate_name(name)?;

        let fields = &config.fields;
        let private_key = fields
            .get("private_key")
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty())
            .map(|s| s.trim().to_string())
            .map_or_else(|| self.generate_private_key(), Ok)?;

        let mut out = String::new();
        out.push_str("[Interface]\n");
        out.push_str(&format!("PrivateKey = {private_key}\n"));

        if let Some(addr) = fields.get("addresses").and_then(|v| v.as_str()) {
            if !addr.trim().is_empty() {
                out.push_str(&format!("Address = {}\n", addr.trim()));
            }
        }
        if let Some(dns) = fields.get("dns").and_then(|v| v.as_str()) {
            if !dns.trim().is_empty() {
                out.push_str(&format!("DNS = {}\n", dns.trim()));
            }
        }
        if let Some(port) = fields.get("listen_port").and_then(|v| v.as_str()) {
            if !port.trim().is_empty() {
                out.push_str(&format!("ListenPort = {}\n", port.trim()));
            }
        }

        if let Some(pubkey) = fields.get("peer_public_key").and_then(|v| v.as_str()) {
            if !pubkey.trim().is_empty() {
                out.push_str("\n[Peer]\n");
                out.push_str(&format!("PublicKey = {}\n", pubkey.trim()));
                if let Some(psk) = fields.get("peer_preshared_key").and_then(|v| v.as_str()) {
                    if !psk.trim().is_empty() {
                        out.push_str(&format!("PresharedKey = {}\n", psk.trim()));
                    }
                }
                if let Some(ips) = fields.get("peer_allowed_ips").and_then(|v| v.as_str()) {
                    if !ips.trim().is_empty() {
                        out.push_str(&format!("AllowedIPs = {}\n", ips.trim()));
                    }
                }
                if let Some(ep) = fields.get("peer_endpoint").and_then(|v| v.as_str()) {
                    if !ep.trim().is_empty() {
                        out.push_str(&format!("Endpoint = {}\n", ep.trim()));
                    }
                }
                if let Some(ka) = fields.get("peer_keepalive").and_then(|v| v.as_str()) {
                    if !ka.trim().is_empty() {
                        out.push_str(&format!("PersistentKeepalive = {}\n", ka.trim()));
                    }
                }
            }
        }

        Ok(out)
    }
}

impl VpnProvider for WireGuardProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::WireGuard
    }

    fn display_name(&self) -> &str {
        "WireGuard"
    }

    fn is_available(&self) -> bool {
        command_exists("wg") && command_exists("wg-quick")
    }

    fn install_hint(&self) -> &str {
        "sudo apt install wireguard-tools (Debian/Ubuntu) or sudo pacman -S wireguard-tools (Arch)"
    }

    fn list_connections(&self) -> Result<Vec<VpnConnection>> {
        let active = self.active_interfaces();
        let mut by_name: BTreeMap<String, VpnConnection> = BTreeMap::new();

        for (name, path) in self.read_config_files()? {
            let is_active = active.contains(&name);
            by_name.insert(
                name.clone(),
                VpnConnection {
                    id: name.clone(),
                    provider: ProviderKind::WireGuard,
                    name: name.clone(),
                    status: if is_active {
                        ConnectionStatus::Connected
                    } else {
                        ConnectionStatus::Disconnected
                    },
                    autostart: self.is_enabled(&name),
                    details: serde_json::json!({ "path": path.display().to_string() }),
                },
            );
        }

        for name in active {
            by_name
                .entry(name.clone())
                .or_insert_with(|| VpnConnection {
                    id: name.clone(),
                    provider: ProviderKind::WireGuard,
                    name: name.clone(),
                    status: ConnectionStatus::Connected,
                    autostart: self.is_enabled(&name),
                    details: serde_json::json!({}),
                });
        }

        Ok(by_name.into_values().collect())
    }

    fn connect(&self, id: &str) -> Result<()> {
        Self::validate_name(id)?;
        if command_exists("systemctl") {
            run(
                "systemctl",
                &["start", &format!("wg-quick@{id}.service")],
                true,
            )
        } else {
            let path = self.config_path(id);
            run("wg-quick", &["up", path.to_string_lossy().as_ref()], true)
        }
        .with_context(|| format!("failed to connect {id}"))?;
        Ok(())
    }

    fn disconnect(&self, id: &str) -> Result<()> {
        Self::validate_name(id)?;
        if command_exists("systemctl") {
            run(
                "systemctl",
                &["stop", &format!("wg-quick@{id}.service")],
                true,
            )
        } else {
            let path = self.config_path(id);
            run("wg-quick", &["down", path.to_string_lossy().as_ref()], true)
        }
        .with_context(|| format!("failed to disconnect {id}"))?;
        Ok(())
    }

    fn status(&self, id: &str) -> Result<ConnectionStatus> {
        let active = self.active_interfaces();
        Ok(if active.contains(id) {
            ConnectionStatus::Connected
        } else {
            ConnectionStatus::Disconnected
        })
    }

    fn status_details(&self, id: &str) -> Result<String> {
        Self::validate_name(id)?;
        run("wg", &["show", id], true)
            .or_else(|_| run("wg", &["show", id], false))
            .unwrap_or_else(|_| "interface not active".to_string());
        let mut lines = Vec::new();
        lines.push(format!("Interface: {id}"));
        if let Ok(output) = run("wg", &["show", id], true) {
            if !output.is_empty() {
                lines.push(output);
            }
        }
        Ok(lines.join("\n"))
    }

    fn create(&self, config: &CreateRequest) -> Result<()> {
        Self::validate_name(&config.name)?;
        let path = self.config_path(&config.name);
        let config_text = self.build_config_text(config)?;

        if run("test", &["-e", path.to_string_lossy().as_ref()], true).is_ok() {
            bail!("{} already exists", path.display());
        }

        run(
            "mkdir",
            &["-p", self.config_dir.to_string_lossy().as_ref()],
            true,
        )?;
        run_with_stdin(
            "tee",
            &[path.to_string_lossy().as_ref()],
            true,
            Some(config_text.as_bytes()),
        )?;
        run("chmod", &["600", path.to_string_lossy().as_ref()], true)?;

        let autostart = config
            .fields
            .get("autostart")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if autostart && command_exists("systemctl") {
            self.set_autostart(&config.name, true)?;
        }

        Ok(())
    }

    fn remove(&self, id: &str) -> Result<()> {
        Self::validate_name(id)?;
        let _ = self.disconnect(id);
        let _ = self.set_autostart(id, false);
        run(
            "rm",
            &["-f", self.config_path(id).to_string_lossy().as_ref()],
            true,
        )
        .with_context(|| format!("failed to remove {id}"))?;
        Ok(())
    }

    fn import(&self, path: &str) -> Result<String> {
        let source = mvpn_core::security::validate_import_path(path)?;
        let name = source
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| anyhow!("cannot determine interface name from path"))?
            .to_string();
        Self::validate_name(&name)?;

        let dest = self.config_path(&name);
        if run("test", &["-e", dest.to_string_lossy().as_ref()], true).is_ok() {
            bail!("{} already exists", dest.display());
        }

        let content = run("cat", &[path.trim()], false)
            .or_else(|_| run("cat", &[path.trim()], true))
            .with_context(|| format!("failed to read {path}"))?;

        run(
            "mkdir",
            &["-p", self.config_dir.to_string_lossy().as_ref()],
            true,
        )?;
        run_with_stdin(
            "tee",
            &[dest.to_string_lossy().as_ref()],
            true,
            Some(content.as_bytes()),
        )?;
        run("chmod", &["600", dest.to_string_lossy().as_ref()], true)?;

        Ok(name)
    }

    fn set_autostart(&self, id: &str, enabled: bool) -> Result<()> {
        Self::validate_name(id)?;
        if !command_exists("systemctl") {
            bail!("systemctl is not available");
        }
        let action = if enabled { "enable" } else { "disable" };
        run(
            "systemctl",
            &[action, &format!("wg-quick@{id}.service")],
            true,
        )
        .with_context(|| format!("failed to {action} autostart for {id}"))?;
        Ok(())
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField {
                key: "addresses".into(),
                label: "Addresses".into(),
                required: false,
                field_type: FieldType::Csv,
            },
            FormField {
                key: "dns".into(),
                label: "DNS".into(),
                required: false,
                field_type: FieldType::Csv,
            },
            FormField {
                key: "private_key".into(),
                label: "Private Key".into(),
                required: false,
                field_type: FieldType::Secret,
            },
            FormField {
                key: "listen_port".into(),
                label: "Listen Port".into(),
                required: false,
                field_type: FieldType::Text,
            },
            FormField {
                key: "peer_public_key".into(),
                label: "Peer Public Key".into(),
                required: false,
                field_type: FieldType::Text,
            },
            FormField {
                key: "peer_preshared_key".into(),
                label: "Peer Preshared Key".into(),
                required: false,
                field_type: FieldType::Secret,
            },
            FormField {
                key: "peer_allowed_ips".into(),
                label: "Peer Allowed IPs".into(),
                required: false,
                field_type: FieldType::Csv,
            },
            FormField {
                key: "peer_endpoint".into(),
                label: "Peer Endpoint".into(),
                required: false,
                field_type: FieldType::Text,
            },
            FormField {
                key: "peer_keepalive".into(),
                label: "Peer Keepalive".into(),
                required: false,
                field_type: FieldType::Text,
            },
            FormField {
                key: "autostart".into(),
                label: "Autostart".into(),
                required: false,
                field_type: FieldType::Bool,
            },
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_name_ok() {
        assert!(WireGuardProvider::validate_name("wg0").is_ok());
        assert!(WireGuardProvider::validate_name("my-vpn").is_ok());
        assert!(WireGuardProvider::validate_name("test_123").is_ok());
        assert!(WireGuardProvider::validate_name("a.b").is_ok());
    }

    #[test]
    fn validate_name_empty() {
        assert!(WireGuardProvider::validate_name("").is_err());
    }

    #[test]
    fn validate_name_invalid_chars() {
        assert!(WireGuardProvider::validate_name("wg 0").is_err());
        assert!(WireGuardProvider::validate_name("wg/0").is_err());
        assert!(WireGuardProvider::validate_name("wg;rm").is_err());
        assert!(WireGuardProvider::validate_name("$(evil)").is_err());
    }

    #[test]
    fn config_path() {
        let p = WireGuardProvider::new();
        assert_eq!(
            p.config_path("wg0"),
            PathBuf::from("/etc/wireguard/wg0.conf")
        );
    }

    fn make_fields(
        entries: &[(&str, serde_json::Value)],
    ) -> serde_json::Map<String, serde_json::Value> {
        entries
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect()
    }

    #[test]
    fn build_config_text_minimal() {
        let p = WireGuardProvider::new();
        let req = CreateRequest {
            name: "wg0".into(),
            fields: make_fields(&[("private_key", serde_json::json!("testkey123"))]),
        };
        let text = p.build_config_text(&req).unwrap();
        assert!(text.contains("[Interface]"));
        assert!(text.contains("PrivateKey = testkey123"));
        assert!(!text.contains("[Peer]"));
    }

    #[test]
    fn build_config_text_full() {
        let p = WireGuardProvider::new();
        let req = CreateRequest {
            name: "wg0".into(),
            fields: make_fields(&[
                ("private_key", serde_json::json!("mykey")),
                ("addresses", serde_json::json!("10.0.0.1/24, fd00::1/64")),
                ("dns", serde_json::json!("1.1.1.1, 9.9.9.9")),
                ("listen_port", serde_json::json!("51820")),
                ("peer_public_key", serde_json::json!("peerpub")),
                ("peer_preshared_key", serde_json::json!("peerpsk")),
                ("peer_allowed_ips", serde_json::json!("0.0.0.0/0")),
                ("peer_endpoint", serde_json::json!("vpn.example.com:51820")),
                ("peer_keepalive", serde_json::json!("25")),
            ]),
        };
        let text = p.build_config_text(&req).unwrap();
        assert!(text.contains("PrivateKey = mykey"));
        assert!(text.contains("Address = 10.0.0.1/24, fd00::1/64"));
        assert!(text.contains("DNS = 1.1.1.1, 9.9.9.9"));
        assert!(text.contains("ListenPort = 51820"));
        assert!(text.contains("[Peer]"));
        assert!(text.contains("PublicKey = peerpub"));
        assert!(text.contains("PresharedKey = peerpsk"));
        assert!(text.contains("AllowedIPs = 0.0.0.0/0"));
        assert!(text.contains("Endpoint = vpn.example.com:51820"));
        assert!(text.contains("PersistentKeepalive = 25"));
    }

    #[test]
    fn build_config_text_empty_peer_fields_no_peer_section() {
        let p = WireGuardProvider::new();
        let req = CreateRequest {
            name: "wg0".into(),
            fields: make_fields(&[
                ("private_key", serde_json::json!("key")),
                ("peer_public_key", serde_json::json!("")),
            ]),
        };
        let text = p.build_config_text(&req).unwrap();
        assert!(!text.contains("[Peer]"));
    }

    #[test]
    fn build_config_text_rejects_empty_name() {
        let p = WireGuardProvider::new();
        let req = CreateRequest {
            name: "".into(),
            fields: make_fields(&[("private_key", serde_json::json!("key"))]),
        };
        assert!(p.build_config_text(&req).is_err());
    }

    #[test]
    fn build_config_text_rejects_invalid_name() {
        let p = WireGuardProvider::new();
        let req = CreateRequest {
            name: "bad;name".into(),
            fields: make_fields(&[("private_key", serde_json::json!("key"))]),
        };
        assert!(p.build_config_text(&req).is_err());
    }

    #[test]
    fn build_config_text_trims_values() {
        let p = WireGuardProvider::new();
        let req = CreateRequest {
            name: "wg0".into(),
            fields: make_fields(&[
                ("private_key", serde_json::json!("  mykey  ")),
                ("addresses", serde_json::json!("  10.0.0.1/24  ")),
            ]),
        };
        let text = p.build_config_text(&req).unwrap();
        assert!(text.contains("PrivateKey = mykey"));
        assert!(text.contains("Address = 10.0.0.1/24"));
    }

    #[test]
    fn provider_kind_and_name() {
        let p = WireGuardProvider::new();
        assert_eq!(p.kind(), ProviderKind::WireGuard);
        assert_eq!(p.display_name(), "WireGuard");
    }
}
