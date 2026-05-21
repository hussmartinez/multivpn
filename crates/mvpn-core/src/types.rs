use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderKind {
    WireGuard,
    OpenVpn,
    ProtonVpn,
    Tailscale,
}

impl ProviderKind {
    pub fn all() -> &'static [ProviderKind] {
        &[
            ProviderKind::WireGuard,
            ProviderKind::OpenVpn,
            ProviderKind::ProtonVpn,
            ProviderKind::Tailscale,
        ]
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            ProviderKind::WireGuard => "wireguard",
            ProviderKind::OpenVpn => "openvpn",
            ProviderKind::ProtonVpn => "protonvpn",
            ProviderKind::Tailscale => "tailscale",
        }
    }
}

impl std::fmt::Display for ProviderKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionStatus {
    Connected,
    Disconnected,
    Connecting,
    Error(String),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VpnConnection {
    pub id: String,
    pub provider: ProviderKind,
    pub name: String,
    pub status: ConnectionStatus,
    pub autostart: bool,
    pub details: serde_json::Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProviderInfo {
    pub kind: ProviderKind,
    pub display_name: String,
    pub available: bool,
    pub install_hint: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FormField {
    pub key: String,
    pub label: String,
    pub required: bool,
    pub field_type: FieldType,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum FieldType {
    Text,
    Secret,
    Bool,
    Csv,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CreateRequest {
    pub name: String,
    pub fields: serde_json::Map<String, serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_kind_all_returns_four() {
        assert_eq!(ProviderKind::all().len(), 4);
    }

    #[test]
    fn provider_kind_as_str_roundtrip() {
        for kind in ProviderKind::all() {
            let s = kind.as_str();
            let json = serde_json::to_string(kind).unwrap();
            assert_eq!(json, format!("\"{s}\""));
            let back: ProviderKind = serde_json::from_str(&json).unwrap();
            assert_eq!(&back, kind);
        }
    }

    #[test]
    fn provider_kind_display() {
        assert_eq!(ProviderKind::WireGuard.to_string(), "wireguard");
        assert_eq!(ProviderKind::Tailscale.to_string(), "tailscale");
    }

    #[test]
    fn connection_status_serde() {
        let s = ConnectionStatus::Connected;
        let json = serde_json::to_string(&s).unwrap();
        let back: ConnectionStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(back, ConnectionStatus::Connected);

        let err = ConnectionStatus::Error("timeout".into());
        let json = serde_json::to_string(&err).unwrap();
        let back: ConnectionStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(back, ConnectionStatus::Error("timeout".into()));
    }

    #[test]
    fn vpn_connection_serde() {
        let conn = VpnConnection {
            id: "wg0".into(),
            provider: ProviderKind::WireGuard,
            name: "wg0".into(),
            status: ConnectionStatus::Connected,
            autostart: true,
            details: serde_json::json!({"path": "/etc/wireguard/wg0.conf"}),
        };
        let json = serde_json::to_string(&conn).unwrap();
        let back: VpnConnection = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "wg0");
        assert_eq!(back.provider, ProviderKind::WireGuard);
        assert_eq!(back.status, ConnectionStatus::Connected);
        assert!(back.autostart);
    }

    #[test]
    fn create_request_default() {
        let req = CreateRequest::default();
        assert!(req.name.is_empty());
        assert!(req.fields.is_empty());
    }

    #[test]
    fn form_field_serde() {
        let field = FormField {
            key: "addr".into(),
            label: "Address".into(),
            required: true,
            field_type: FieldType::Csv,
        };
        let json = serde_json::to_string(&field).unwrap();
        let back: FormField = serde_json::from_str(&json).unwrap();
        assert_eq!(back.key, "addr");
        assert!(back.required);
    }
}
