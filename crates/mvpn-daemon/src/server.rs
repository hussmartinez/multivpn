use crate::killswitch;
use crate::state::DaemonState;
use anyhow::Result;
use mvpn_core::ipc::{self, Request, Response};
use mvpn_core::types::{ProviderInfo, ProviderKind};
use mvpn_providers::all_providers;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::RwLock;

pub async fn handle_client(stream: UnixStream, state: Arc<RwLock<DaemonState>>) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    while let Some(line) = lines.next_line().await? {
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
    match req {
        Request::ListConnections => {
            let mut all = Vec::new();
            for provider in all_providers() {
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
            match killswitch::enable() {
                Ok(()) => {
                    s.kill_switch_active = true;
                    s.config.general.kill_switch = true;
                    let _ = s.config.save();
                    Response::KillSwitch { active: true }
                }
                Err(e) => Response::Error {
                    message: e.to_string(),
                },
            }
        }

        Request::KillSwitchDisable => {
            let mut s = state.write().await;
            match killswitch::disable() {
                Ok(()) => {
                    s.kill_switch_active = false;
                    s.config.general.kill_switch = false;
                    let _ = s.config.save();
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

        Request::ListProviders => {
            let items: Vec<ProviderInfo> = ProviderKind::all()
                .iter()
                .map(|kind| {
                    let p = mvpn_providers::create_provider(*kind);
                    ProviderInfo {
                        kind: *kind,
                        display_name: p.display_name().to_string(),
                        available: p.is_available(),
                        install_hint: p.install_hint().to_string(),
                    }
                })
                .collect();
            Response::Providers { items }
        }
    }
}
