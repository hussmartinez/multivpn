use crate::command::{command_exists, run};
use anyhow::{Context, Result, bail};
use mvpn_core::provider::VpnProvider;
use mvpn_core::types::*;

enum ProtonTool {
    ProtonVpnCli,
    ProtonVpnGtk,
}

pub struct ProtonVpnProvider {
    tool: Option<ProtonTool>,
}

impl ProtonVpnProvider {
    pub fn new() -> Self {
        let tool = if command_exists("protonvpn-cli") {
            Some(ProtonTool::ProtonVpnCli)
        } else if command_exists("protonvpn") {
            Some(ProtonTool::ProtonVpnCli)
        } else if command_exists("proton-vpn-gtk-app") {
            Some(ProtonTool::ProtonVpnGtk)
        } else {
            None
        };
        Self { tool }
    }

    fn cli_name(&self) -> &str {
        match &self.tool {
            Some(ProtonTool::ProtonVpnCli) => {
                if command_exists("protonvpn-cli") {
                    "protonvpn-cli"
                } else {
                    "protonvpn"
                }
            }
            Some(ProtonTool::ProtonVpnGtk) => "proton-vpn-gtk-app",
            None => "protonvpn-cli",
        }
    }
}

impl VpnProvider for ProtonVpnProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::ProtonVpn
    }

    fn display_name(&self) -> &str {
        "ProtonVPN"
    }

    fn is_available(&self) -> bool {
        self.tool.is_some()
    }

    fn install_hint(&self) -> &str {
        "pip install protonvpn-cli or install from https://protonvpn.com/download-linux"
    }

    fn list_connections(&self) -> Result<Vec<VpnConnection>> {
        if self.tool.is_none() {
            return Ok(Vec::new());
        }

        let status = self.status("default")?;
        Ok(vec![VpnConnection {
            id: "default".into(),
            provider: ProviderKind::ProtonVpn,
            name: "ProtonVPN".into(),
            status,
            autostart: false,
            details: serde_json::json!({}),
        }])
    }

    fn connect(&self, id: &str) -> Result<()> {
        if self.tool.is_none() {
            bail!("ProtonVPN CLI not installed");
        }
        let cli = self.cli_name();
        if id == "fastest" || id == "default" {
            run(cli, &["connect", "--fastest"], false)
        } else {
            run(cli, &["connect", id], false)
        }
        .with_context(|| "failed to connect to ProtonVPN")?;
        Ok(())
    }

    fn disconnect(&self, _id: &str) -> Result<()> {
        if self.tool.is_none() {
            bail!("ProtonVPN CLI not installed");
        }
        run(self.cli_name(), &["disconnect"], false)
            .with_context(|| "failed to disconnect ProtonVPN")?;
        Ok(())
    }

    fn status(&self, _id: &str) -> Result<ConnectionStatus> {
        if self.tool.is_none() {
            return Ok(ConnectionStatus::Disconnected);
        }
        let output = run(self.cli_name(), &["status"], false).unwrap_or_default();
        if output.contains("Connected") || output.contains("connected") {
            Ok(ConnectionStatus::Connected)
        } else {
            Ok(ConnectionStatus::Disconnected)
        }
    }

    fn status_details(&self, _id: &str) -> Result<String> {
        if self.tool.is_none() {
            return Ok("ProtonVPN CLI not installed".into());
        }
        run(self.cli_name(), &["status"], false).or_else(|_| Ok("unable to get status".into()))
    }

    fn create(&self, _config: &CreateRequest) -> Result<()> {
        bail!("ProtonVPN connections are managed through the ProtonVPN CLI login")
    }

    fn remove(&self, _id: &str) -> Result<()> {
        bail!("ProtonVPN connections cannot be individually removed")
    }

    fn import(&self, _path: &str) -> Result<String> {
        bail!("ProtonVPN does not support importing config files directly")
    }

    fn set_autostart(&self, _id: &str, _enabled: bool) -> Result<()> {
        bail!("ProtonVPN autostart is managed through the ProtonVPN app settings")
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField {
            key: "server".into(),
            label: "Server (country code or 'fastest')".into(),
            required: false,
            field_type: FieldType::Text,
        }]
    }
}
