use crate::types::{
    ConnectionStatus, CreateRequest, FormField, ProviderInfo, ProviderKind, VpnConnection,
};
use serde::{Deserialize, Serialize};

pub const SOCKET_PATH: &str = "/run/multivpn.sock";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Request {
    ListConnections,
    Connect {
        provider: ProviderKind,
        id: String,
    },
    Disconnect {
        provider: ProviderKind,
        id: String,
    },
    Status {
        provider: ProviderKind,
        id: String,
    },
    Create {
        provider: ProviderKind,
        config: CreateRequest,
    },
    Remove {
        provider: ProviderKind,
        id: String,
    },
    Import {
        provider: ProviderKind,
        path: String,
    },
    GetConfigFields {
        provider: ProviderKind,
    },
    SetAutostart {
        provider: ProviderKind,
        id: String,
        enabled: bool,
    },
    KillSwitchEnable,
    KillSwitchDisable,
    KillSwitchStatus,
    ListProviders,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Response {
    Ok {
        message: String,
    },
    Error {
        message: String,
    },
    Connections {
        items: Vec<VpnConnection>,
    },
    Status {
        provider: ProviderKind,
        id: String,
        status: ConnectionStatus,
        details: String,
    },
    KillSwitch {
        active: bool,
    },
    Providers {
        items: Vec<ProviderInfo>,
    },
    ConfigFields {
        provider: ProviderKind,
        fields: Vec<FormField>,
    },
}

pub fn encode(msg: &impl Serialize) -> anyhow::Result<String> {
    let mut line = serde_json::to_string(msg)?;
    line.push('\n');
    Ok(line)
}

pub fn decode_request(line: &str) -> anyhow::Result<Request> {
    Ok(serde_json::from_str(line.trim())?)
}

pub fn decode_response(line: &str) -> anyhow::Result<Response> {
    Ok(serde_json::from_str(line.trim())?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_ends_with_newline() {
        let req = Request::ListConnections;
        let encoded = encode(&req).unwrap();
        assert!(encoded.ends_with('\n'));
    }

    #[test]
    fn request_roundtrip_list() {
        let req = Request::ListConnections;
        let encoded = encode(&req).unwrap();
        let decoded = decode_request(&encoded).unwrap();
        assert!(matches!(decoded, Request::ListConnections));
    }

    #[test]
    fn request_roundtrip_connect() {
        let req = Request::Connect {
            provider: ProviderKind::WireGuard,
            id: "wg0".into(),
        };
        let encoded = encode(&req).unwrap();
        let decoded = decode_request(&encoded).unwrap();
        match decoded {
            Request::Connect { provider, id } => {
                assert_eq!(provider, ProviderKind::WireGuard);
                assert_eq!(id, "wg0");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn request_roundtrip_killswitch() {
        for req in [
            Request::KillSwitchEnable,
            Request::KillSwitchDisable,
            Request::KillSwitchStatus,
        ] {
            let encoded = encode(&req).unwrap();
            let decoded = decode_request(&encoded).unwrap();
            // Just verify it doesn't error
            let _ = decoded;
        }
    }

    #[test]
    fn request_roundtrip_create() {
        let mut fields = serde_json::Map::new();
        fields.insert("addresses".into(), serde_json::json!("10.0.0.1/24"));
        let req = Request::Create {
            provider: ProviderKind::WireGuard,
            config: CreateRequest {
                name: "wg0".into(),
                fields,
            },
        };
        let encoded = encode(&req).unwrap();
        let decoded = decode_request(&encoded).unwrap();
        match decoded {
            Request::Create { provider, config } => {
                assert_eq!(provider, ProviderKind::WireGuard);
                assert_eq!(config.name, "wg0");
                assert!(config.fields.contains_key("addresses"));
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn request_roundtrip_set_autostart() {
        let req = Request::SetAutostart {
            provider: ProviderKind::Tailscale,
            id: "default".into(),
            enabled: true,
        };
        let encoded = encode(&req).unwrap();
        let decoded = decode_request(&encoded).unwrap();
        match decoded {
            Request::SetAutostart {
                provider,
                id,
                enabled,
            } => {
                assert_eq!(provider, ProviderKind::Tailscale);
                assert_eq!(id, "default");
                assert!(enabled);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn response_roundtrip_ok() {
        let resp = Response::Ok {
            message: "done".into(),
        };
        let encoded = encode(&resp).unwrap();
        let decoded = decode_response(&encoded).unwrap();
        match decoded {
            Response::Ok { message } => assert_eq!(message, "done"),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn response_roundtrip_error() {
        let resp = Response::Error {
            message: "not found".into(),
        };
        let encoded = encode(&resp).unwrap();
        let decoded = decode_response(&encoded).unwrap();
        match decoded {
            Response::Error { message } => assert_eq!(message, "not found"),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn response_roundtrip_connections() {
        let resp = Response::Connections {
            items: vec![VpnConnection {
                id: "wg0".into(),
                provider: ProviderKind::WireGuard,
                name: "wg0".into(),
                status: ConnectionStatus::Connected,
                autostart: false,
                details: serde_json::json!({}),
            }],
        };
        let encoded = encode(&resp).unwrap();
        let decoded = decode_response(&encoded).unwrap();
        match decoded {
            Response::Connections { items } => {
                assert_eq!(items.len(), 1);
                assert_eq!(items[0].id, "wg0");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn response_roundtrip_killswitch() {
        let resp = Response::KillSwitch { active: true };
        let encoded = encode(&resp).unwrap();
        let decoded = decode_response(&encoded).unwrap();
        match decoded {
            Response::KillSwitch { active } => assert!(active),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn response_roundtrip_providers() {
        let resp = Response::Providers {
            items: vec![ProviderInfo {
                kind: ProviderKind::OpenVpn,
                display_name: "OpenVPN".into(),
                available: true,
                install_hint: "apt install openvpn".into(),
            }],
        };
        let encoded = encode(&resp).unwrap();
        let decoded = decode_response(&encoded).unwrap();
        match decoded {
            Response::Providers { items } => {
                assert_eq!(items.len(), 1);
                assert_eq!(items[0].kind, ProviderKind::OpenVpn);
                assert!(items[0].available);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn request_roundtrip_get_config_fields() {
        let req = Request::GetConfigFields {
            provider: ProviderKind::WireGuard,
        };
        let encoded = encode(&req).unwrap();
        let decoded = decode_request(&encoded).unwrap();
        match decoded {
            Request::GetConfigFields { provider } => {
                assert_eq!(provider, ProviderKind::WireGuard);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn response_roundtrip_config_fields() {
        let resp = Response::ConfigFields {
            provider: ProviderKind::OpenVpn,
            fields: vec![FormField {
                key: "config_path".into(),
                label: "Config Path".into(),
                required: true,
                field_type: crate::types::FieldType::Text,
            }],
        };
        let encoded = encode(&resp).unwrap();
        let decoded = decode_response(&encoded).unwrap();
        match decoded {
            Response::ConfigFields { provider, fields } => {
                assert_eq!(provider, ProviderKind::OpenVpn);
                assert_eq!(fields.len(), 1);
                assert_eq!(fields[0].key, "config_path");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn decode_request_trims_whitespace() {
        let req = Request::ListProviders;
        let mut encoded = encode(&req).unwrap();
        encoded.push_str("  \n");
        let decoded = decode_request(&encoded).unwrap();
        assert!(matches!(decoded, Request::ListProviders));
    }

    #[test]
    fn decode_invalid_json_errors() {
        assert!(decode_request("not json").is_err());
        assert!(decode_response("{bad}").is_err());
    }
}
