use crate::command::{command_exists, run};
use anyhow::{Context, Result, bail};
use mvpn_core::provider::VpnProvider;
use mvpn_core::types::*;

pub struct TailscaleProvider;

impl TailscaleProvider {
    pub fn new() -> Self {
        Self
    }
}

impl VpnProvider for TailscaleProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Tailscale
    }

    fn display_name(&self) -> &str {
        "Tailscale"
    }

    fn is_available(&self) -> bool {
        command_exists("tailscale")
    }

    fn install_hint(&self) -> &str {
        "curl -fsSL https://tailscale.com/install.sh | sh"
    }

    fn list_connections(&self) -> Result<Vec<VpnConnection>> {
        if !self.is_available() {
            return Ok(Vec::new());
        }

        let status = self.status("default")?;
        let details = self.parse_status_details();
        let network = if matches!(status, ConnectionStatus::Connected) {
            self.gather_network_info()
        } else {
            NetworkInfo::default()
        };

        Ok(vec![VpnConnection {
            id: "default".into(),
            provider: ProviderKind::Tailscale,
            name: "Tailscale".into(),
            status,
            autostart: self.is_enabled(),
            details,
            network,
        }])
    }

    fn connect(&self, _id: &str) -> Result<()> {
        run("tailscale", &["up"], true)
            .or_else(|_| run("tailscale", &["up"], false))
            .with_context(|| "failed to connect Tailscale")?;
        Ok(())
    }

    fn disconnect(&self, _id: &str) -> Result<()> {
        run("tailscale", &["down"], true)
            .or_else(|_| run("tailscale", &["down"], false))
            .with_context(|| "failed to disconnect Tailscale")?;
        Ok(())
    }

    fn status(&self, _id: &str) -> Result<ConnectionStatus> {
        let output = run("tailscale", &["status", "--json"], false)
            .or_else(|_| run("tailscale", &["status", "--json"], true))
            .unwrap_or_default();

        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&output) {
            if let Some(state) = json.get("BackendState").and_then(|v| v.as_str()) {
                return Ok(match state {
                    "Running" => ConnectionStatus::Connected,
                    "Starting" => ConnectionStatus::Connecting,
                    _ => ConnectionStatus::Disconnected,
                });
            }
        }
        Ok(ConnectionStatus::Disconnected)
    }

    fn status_details(&self, _id: &str) -> Result<String> {
        run("tailscale", &["status"], false)
            .or_else(|_| run("tailscale", &["status"], true))
            .or_else(|_| Ok("unable to get status".into()))
    }

    fn create(&self, _config: &CreateRequest) -> Result<()> {
        bail!("Tailscale connections are managed through `tailscale up`")
    }

    fn remove(&self, _id: &str) -> Result<()> {
        bail!("Tailscale connections cannot be removed; use `tailscale logout`")
    }

    fn import(&self, _path: &str) -> Result<String> {
        bail!("Tailscale does not use config files")
    }

    fn set_autostart(&self, _id: &str, enabled: bool) -> Result<()> {
        if !command_exists("systemctl") {
            bail!("systemctl is not available");
        }
        let action = if enabled { "enable" } else { "disable" };
        run("systemctl", &[action, "tailscaled.service"], true)
            .with_context(|| format!("failed to {action} Tailscale autostart"))?;
        Ok(())
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![]
    }
}

impl TailscaleProvider {
    fn parse_status_details(&self) -> serde_json::Value {
        let output = match self.status_details("default") {
            Ok(text) => text,
            Err(_) => return serde_json::json!({}),
        };

        let mut peers = Vec::new();
        for line in output.lines().skip(1) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 4 {
                peers.push(serde_json::json!({
                    "ip": parts[0],
                    "hostname": parts[1],
                    "os": parts[2],
                    "status": parts[3..].join(" "),
                }));
            }
        }

        if peers.is_empty() {
            serde_json::json!({})
        } else {
            serde_json::json!({ "peers": peers })
        }
    }

    fn gather_network_info(&self) -> NetworkInfo {
        let mut info = NetworkInfo {
            interface: Some("tailscale0".to_string()),
            ..Default::default()
        };

        let output = run("tailscale", &["status", "--json"], false)
            .or_else(|_| run("tailscale", &["status", "--json"], true))
            .unwrap_or_default();

        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&output) {
            if let Some(self_node) = json.get("Self") {
                if let Some(ips) = self_node.get("TailscaleIPs").and_then(|v| v.as_array()) {
                    info.local_ip = ips.first().and_then(|v| v.as_str()).map(|s| s.to_string());
                }
            }
        }

        if let Ok(dns_output) = run("tailscale", &["dns", "status"], false) {
            info.dns = dns_output.lines()
                .filter(|l| !l.is_empty() && !l.contains(':'))
                .map(|s| s.trim().to_string())
                .collect();
        }

        info
    }

    fn is_enabled(&self) -> bool {
        if !command_exists("systemctl") {
            return false;
        }
        std::process::Command::new("systemctl")
            .args(["is-enabled", "tailscaled.service"])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
}
