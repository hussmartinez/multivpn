use crate::state::DaemonState;
use anyhow::{Result, bail};
use mvpn_core::ipc::{self, Request, Response};
use mvpn_core::types::ProviderInfo;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::RwLock;

const MAX_REQUEST_SIZE: usize = 65536;

pub async fn handle_client(stream: UnixStream, state: Arc<RwLock<DaemonState>>) -> Result<()> {
    let peer_uid = peer_uid(&stream)?;
    if !is_authorized(peer_uid) {
        bail!("unauthorized client uid={peer_uid}");
    }
    handle_client_inner(stream, state).await
}

pub async fn handle_client_unauth(stream: UnixStream, state: Arc<RwLock<DaemonState>>) -> Result<()> {
    handle_client_inner(stream, state).await
}

async fn handle_client_inner(stream: UnixStream, state: Arc<RwLock<DaemonState>>) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    while let Some(line) = lines.next_line().await? {
        if line.len() > MAX_REQUEST_SIZE {
            let response = Response::Error {
                message: "request too large".into(),
            };
            let encoded = ipc::encode(&response)?;
            writer.write_all(encoded.as_bytes()).await?;
            continue;
        }

        let response = match ipc::decode_request(&line) {
            Ok(req) => handle_request(req, &state).await,
            Err(e) => Response::Error {
                message: format!("invalid request: {e}"),
            },
        };

        let encoded = ipc::encode(&response)?;
        writer.write_all(encoded.as_bytes()).await?;
    }

    Ok(())
}

async fn handle_request(req: Request, state: &Arc<RwLock<DaemonState>>) -> Response {
    if let Err(e) = validate_request_ids(&req) {
        return Response::Error {
            message: e.to_string(),
        };
    }

    match req {
        Request::ListConnections => {
            let providers = {
                let s = state.read().await;
                s.providers()
            };
            let mut all = Vec::new();
            for provider in providers {
                if provider.is_available() {
                    match provider.list_connections() {
                        Ok(conns) => all.extend(conns),
                        Err(e) => {
                            return Response::Error {
                                message: format!("{}: {e}", provider.display_name()),
                            };
                        }
                    }
                }
            }
            Response::Connections { items: all }
        }

        Request::Connect { provider, id } => {
            let s = state.read().await;
            let p = s.provider(provider);
            match p.connect(&id) {
                Ok(()) => Response::Ok {
                    message: format!("connected {provider} {id}"),
                },
                Err(e) => Response::Error {
                    message: e.to_string(),
                },
            }
        }

        Request::Disconnect { provider, id } => {
            let s = state.read().await;
            let p = s.provider(provider);
            match p.disconnect(&id) {
                Ok(()) => Response::Ok {
                    message: format!("disconnected {provider} {id}"),
                },
                Err(e) => Response::Error {
                    message: e.to_string(),
                },
            }
        }

        Request::Status { provider, id } => {
            let s = state.read().await;
            let p = s.provider(provider);
            match (p.status(&id), p.status_details(&id)) {
                (Ok(status), Ok(details)) => Response::Status {
                    provider,
                    id,
                    status,
                    details,
                },
                (Err(e), _) | (_, Err(e)) => Response::Error {
                    message: e.to_string(),
                },
            }
        }

        Request::Create { provider, config } => {
            let s = state.read().await;
            let p = s.provider(provider);
            match p.create(&config) {
                Ok(()) => Response::Ok {
                    message: format!("created {}", config.name),
                },
                Err(e) => Response::Error {
                    message: e.to_string(),
                },
            }
        }

        Request::Remove { provider, id } => {
            let s = state.read().await;
            let p = s.provider(provider);
            match p.remove(&id) {
                Ok(()) => Response::Ok {
                    message: format!("removed {provider} {id}"),
                },
                Err(e) => Response::Error {
                    message: e.to_string(),
                },
            }
        }

        Request::Import { provider, path } => {
            let s = state.read().await;
            let p = s.provider(provider);
            match p.import(&path) {
                Ok(name) => Response::Ok {
                    message: format!("imported as {name}"),
                },
                Err(e) => Response::Error {
                    message: e.to_string(),
                },
            }
        }

        Request::GetConfigFields { provider } => {
            let s = state.read().await;
            let p = s.provider(provider);
            Response::ConfigFields {
                provider,
                fields: p.config_fields(),
            }
        }

        Request::SetAutostart {
            provider,
            id,
            enabled,
        } => {
            let s = state.read().await;
            let p = s.provider(provider);
            match p.set_autostart(&id, enabled) {
                Ok(()) => Response::Ok {
                    message: format!(
                        "autostart {} for {provider} {id}",
                        if enabled { "enabled" } else { "disabled" }
                    ),
                },
                Err(e) => Response::Error {
                    message: e.to_string(),
                },
            }
        }

        Request::AutoconnectList => {
            let s = state.read().await;
            Response::AutoconnectEntries {
                items: s.config.autoconnect.connections.clone(),
            }
        }

        Request::AutoconnectAdd { provider, id } => {
            let mut s = state.write().await;
            let entry = mvpn_core::config::AutoconnectEntry {
                provider,
                id: id.clone(),
            };

            if s.config
                .autoconnect
                .connections
                .iter()
                .any(|existing| existing.provider == provider && existing.id == id)
            {
                Response::Ok {
                    message: format!("autoconnect entry already exists for {provider} {id}"),
                }
            } else {
                s.config.autoconnect.connections.push(entry);
                match s.config.save() {
                    Ok(()) => Response::Ok {
                        message: format!("added autoconnect entry for {provider} {id}"),
                    },
                    Err(e) => Response::Error {
                        message: e.to_string(),
                    },
                }
            }
        }

        Request::AutoconnectRemove { provider, id } => {
            let mut s = state.write().await;
            let before = s.config.autoconnect.connections.len();
            s.config
                .autoconnect
                .connections
                .retain(|entry| !(entry.provider == provider && entry.id == id));

            if s.config.autoconnect.connections.len() == before {
                Response::Error {
                    message: format!("autoconnect entry not found for {provider} {id}"),
                }
            } else {
                match s.config.save() {
                    Ok(()) => Response::Ok {
                        message: format!("removed autoconnect entry for {provider} {id}"),
                    },
                    Err(e) => Response::Error {
                        message: e.to_string(),
                    },
                }
            }
        }

        Request::KillSwitchEnable => {
            let mut s = state.write().await;
            match s.kill_switch().enable() {
                Ok(()) => {
                    s.kill_switch_active = true;
                    s.config.general.kill_switch = true;
                    let _ = s.save_config();
                    Response::KillSwitch { active: true }
                }
                Err(e) => Response::Error {
                    message: e.to_string(),
                },
            }
        }

        Request::KillSwitchDisable => {
            let mut s = state.write().await;
            match s.kill_switch().disable() {
                Ok(()) => {
                    s.kill_switch_active = false;
                    s.config.general.kill_switch = false;
                    let _ = s.save_config();
                    Response::KillSwitch { active: false }
                }
                Err(e) => Response::Error {
                    message: e.to_string(),
                },
            }
        }

        Request::KillSwitchStatus => {
            let s = state.read().await;
            Response::KillSwitch {
                active: s.kill_switch_active,
            }
        }

        Request::ConfigGet { key } => {
            let s = state.read().await;
            match s.config.get_value(&key) {
                Ok(value) => Response::ConfigValue { value },
                Err(e) => Response::Error {
                    message: e.to_string(),
                },
            }
        }

        Request::ConfigSet { key, value } => {
            let mut s = state.write().await;
            match s
                .config
                .set_value(&key, &value)
                .and_then(|_| s.config.save())
            {
                Ok(()) => Response::Ok {
                    message: "config updated".into(),
                },
                Err(e) => Response::Error {
                    message: e.to_string(),
                },
            }
        }

        Request::ListProviders => {
            let providers = {
                let s = state.read().await;
                s.providers()
            };
            let items: Vec<ProviderInfo> = providers
                .into_iter()
                .map(|p| ProviderInfo {
                    kind: p.kind(),
                    display_name: p.display_name().to_string(),
                    available: p.is_available(),
                    install_hint: p.install_hint().to_string(),
                })
                .collect();
            Response::Providers { items }
        }
    }
}

fn validate_request_ids(req: &Request) -> anyhow::Result<()> {
    use mvpn_core::security::validate_connection_id;
    match req {
        Request::Connect { id, .. }
        | Request::Disconnect { id, .. }
        | Request::Status { id, .. }
        | Request::Remove { id, .. }
        | Request::SetAutostart { id, .. }
        | Request::AutoconnectAdd { id, .. }
        | Request::AutoconnectRemove { id, .. } => validate_connection_id(id)?,
        Request::Import { path, .. } => {
            mvpn_core::security::validate_import_path(path)?;
        }
        _ => {}
    }
    Ok(())
}

fn peer_uid(stream: &UnixStream) -> Result<u32> {
    let cred = stream.peer_cred()?;
    Ok(cred.uid())
}

fn is_authorized(uid: u32) -> bool {
    if uid == 0 {
        return true;
    }
    is_in_multivpn_group(uid)
}

fn is_in_multivpn_group(uid: u32) -> bool {
    let group_name = "multivpn";
    let Ok(output) = std::process::Command::new("id")
        .arg("-Gn")
        .arg(uid.to_string())
        .output()
    else {
        return false;
    };
    let groups = String::from_utf8_lossy(&output.stdout);
    groups.split_whitespace().any(|g| g == group_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_is_always_authorized() {
        assert!(is_authorized(0));
    }

    #[test]
    fn max_request_size_is_64k() {
        assert_eq!(MAX_REQUEST_SIZE, 65536);
    }
}
