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
        let details = self.status_details("default").unwrap_or_default();

        Ok(vec![VpnConnection {
            id: "default".into(),
            provider: ProviderKind::Tailscale,
            name: "Tailscale".into(),
            status,
            autostart: self.is_enabled(),
            details: serde_json::json!({ "status": details }),
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
