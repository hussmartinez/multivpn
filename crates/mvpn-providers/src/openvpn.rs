use crate::command::{command_exists, run};
use anyhow::{Context, Result, bail};
use mvpn_core::provider::VpnProvider;
use mvpn_core::types::*;
use std::path::PathBuf;

pub struct OpenVpnProvider {
    config_dir: PathBuf,
}

impl OpenVpnProvider {
    pub fn new() -> Self {
        Self {
            config_dir: PathBuf::from("/etc/openvpn"),
        }
    }
}

impl VpnProvider for OpenVpnProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::OpenVpn
    }

    fn display_name(&self) -> &str {
        "OpenVPN"
    }

    fn is_available(&self) -> bool {
        command_exists("openvpn")
    }

    fn install_hint(&self) -> &str {
        "sudo apt install openvpn (Debian/Ubuntu) or sudo pacman -S openvpn (Arch)"
    }

    fn list_connections(&self) -> Result<Vec<VpnConnection>> {
        let mut connections = Vec::new();
        let mut paths = Vec::new();
        Self::collect_configs(&self.config_dir, 2, &mut paths);
        paths.sort();

        for path in paths {
            let name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string();

            let is_active = self.is_service_active(&name);
            connections.push(VpnConnection {
                id: name.clone(),
                provider: ProviderKind::OpenVpn,
                name: name.clone(),
                status: if is_active {
                    ConnectionStatus::Connected
                } else {
                    ConnectionStatus::Disconnected
                },
                autostart: self.is_enabled(&name),
                details: serde_json::json!({ "path": path.display().to_string() }),
            });
        }
        Ok(connections)
    }

    fn connect(&self, id: &str) -> Result<()> {
        if command_exists("systemctl") {
            run(
                "systemctl",
                &["start", &format!("openvpn@{id}.service")],
                true,
            )
            .with_context(|| format!("failed to connect {id}"))?;
        } else {
            bail!("systemctl required for OpenVPN service management");
        }
        Ok(())
    }

    fn disconnect(&self, id: &str) -> Result<()> {
        if command_exists("systemctl") {
            run(
                "systemctl",
                &["stop", &format!("openvpn@{id}.service")],
                true,
            )
            .with_context(|| format!("failed to disconnect {id}"))?;
        } else {
            bail!("systemctl required for OpenVPN service management");
        }
        Ok(())
    }

    fn status(&self, id: &str) -> Result<ConnectionStatus> {
        Ok(if self.is_service_active(id) {
            ConnectionStatus::Connected
        } else {
            ConnectionStatus::Disconnected
        })
    }

    fn status_details(&self, id: &str) -> Result<String> {
        if command_exists("systemctl") {
            run(
                "systemctl",
                &["status", "--no-pager", &format!("openvpn@{id}.service")],
                true,
            )
            .or_else(|_| Ok("service not found".to_string()))
        } else {
            Ok("systemctl not available".to_string())
        }
    }

    fn create(&self, _config: &CreateRequest) -> Result<()> {
        bail!("OpenVPN configs should be imported from .ovpn/.conf files")
    }

    fn remove(&self, id: &str) -> Result<()> {
        let _ = self.disconnect(id);
        let _ = self.set_autostart(id, false);
        let conf = self.config_dir.join(format!("{id}.conf"));
        run("rm", &["-f", conf.to_string_lossy().as_ref()], true)?;
        Ok(())
    }

    fn import(&self, path: &str) -> Result<String> {
        let source = std::path::Path::new(path.trim());
        let name = source
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| anyhow::anyhow!("cannot determine name from path"))?
            .to_string();

        let dest = self.config_dir.join(format!("{name}.conf"));
        run(
            "mkdir",
            &["-p", self.config_dir.to_string_lossy().as_ref()],
            true,
        )?;
        run("cp", &[path.trim(), dest.to_string_lossy().as_ref()], true)
            .with_context(|| format!("failed to import {path}"))?;
        run("chmod", &["600", dest.to_string_lossy().as_ref()], true)?;
        Ok(name)
    }

    fn set_autostart(&self, id: &str, enabled: bool) -> Result<()> {
        if !command_exists("systemctl") {
            bail!("systemctl is not available");
        }
        let action = if enabled { "enable" } else { "disable" };
        run(
            "systemctl",
            &[action, &format!("openvpn@{id}.service")],
            true,
        )
        .with_context(|| format!("failed to {action} autostart for {id}"))?;
        Ok(())
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField {
                key: "config_path".into(),
                label: "Config File Path".into(),
                required: true,
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

impl OpenVpnProvider {
    fn collect_configs(dir: &PathBuf, depth: usize, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() && depth > 1 {
                Self::collect_configs(&path, depth - 1, out);
            } else if path.is_file() {
                if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                    if ext == "conf" || ext == "ovpn" {
                        out.push(path);
                    }
                }
            }
        }
    }

    fn is_service_active(&self, name: &str) -> bool {
        if !command_exists("systemctl") {
            return false;
        }
        std::process::Command::new("sudo")
            .args([
                "-n",
                "systemctl",
                "is-active",
                &format!("openvpn@{name}.service"),
            ])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    fn is_enabled(&self, name: &str) -> bool {
        if !command_exists("systemctl") {
            return false;
        }
        std::process::Command::new("sudo")
            .args([
                "-n",
                "systemctl",
                "is-enabled",
                &format!("openvpn@{name}.service"),
            ])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
}
