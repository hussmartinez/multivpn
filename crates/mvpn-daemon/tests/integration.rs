use anyhow::{Result, anyhow};
use mvpn_core::config::Config;
use mvpn_core::ipc::{Request, Response, decode_response, encode};
use mvpn_core::provider::VpnProvider;
use mvpn_core::types::{ConnectionStatus, CreateRequest, FormField, ProviderKind, VpnConnection};
use mvpn_daemon::killswitch::KillSwitchController;
use mvpn_daemon::server::handle_client;
use mvpn_daemon::state::{DaemonState, StaticProviderRegistry};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tempfile::TempDir;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::RwLock;

#[derive(Clone, Debug, PartialEq, Eq)]
enum ProviderCall {
    ListConnections,
    Connect(String),
    Disconnect(String),
    Status(String),
    StatusDetails(String),
    SetAutostart { id: String, enabled: bool },
}

#[derive(Default)]
struct MockProviderData {
    connections: Vec<VpnConnection>,
    details: HashMap<String, String>,
    calls: Vec<ProviderCall>,
}

struct MockProvider {
    kind: ProviderKind,
    display_name: &'static str,
    install_hint: &'static str,
    data: Arc<Mutex<MockProviderData>>,
}

impl MockProvider {
    fn new(kind: ProviderKind, display_name: &'static str, connections: Vec<VpnConnection>) -> Self {
        let details = connections
            .iter()
            .map(|conn| (conn.id.clone(), format!("details for {}", conn.id)))
            .collect();
        Self {
            kind,
            display_name,
            install_hint: "mock install hint",
            data: Arc::new(Mutex::new(MockProviderData {
                connections,
                details,
                calls: Vec::new(),
            })),
        }
    }

    fn calls(&self) -> Vec<ProviderCall> {
        self.data.lock().unwrap().calls.clone()
    }
}

impl VpnProvider for MockProvider {
    fn kind(&self) -> ProviderKind {
        self.kind
    }

    fn display_name(&self) -> &str {
        self.display_name
    }

    fn is_available(&self) -> bool {
        true
    }

    fn install_hint(&self) -> &str {
        self.install_hint
    }

    fn list_connections(&self) -> Result<Vec<VpnConnection>> {
        let mut data = self.data.lock().unwrap();
        data.calls.push(ProviderCall::ListConnections);
        Ok(data.connections.clone())
    }

    fn connect(&self, id: &str) -> Result<()> {
        let mut data = self.data.lock().unwrap();
        data.calls.push(ProviderCall::Connect(id.to_string()));
        let conn = data
            .connections
            .iter_mut()
            .find(|conn| conn.id == id)
            .ok_or_else(|| anyhow!("connection not found: {id}"))?;
        conn.status = ConnectionStatus::Connected;
        Ok(())
    }

    fn disconnect(&self, id: &str) -> Result<()> {
        let mut data = self.data.lock().unwrap();
        data.calls.push(ProviderCall::Disconnect(id.to_string()));
        let conn = data
            .connections
            .iter_mut()
            .find(|conn| conn.id == id)
            .ok_or_else(|| anyhow!("connection not found: {id}"))?;
        conn.status = ConnectionStatus::Disconnected;
        Ok(())
    }

    fn status(&self, id: &str) -> Result<ConnectionStatus> {
        let mut data = self.data.lock().unwrap();
        data.calls.push(ProviderCall::Status(id.to_string()));
        data.connections
            .iter()
            .find(|conn| conn.id == id)
            .map(|conn| conn.status.clone())
            .ok_or_else(|| anyhow!("connection not found: {id}"))
    }

    fn status_details(&self, id: &str) -> Result<String> {
        let mut data = self.data.lock().unwrap();
        data.calls.push(ProviderCall::StatusDetails(id.to_string()));
        data.details
            .get(id)
            .cloned()
            .ok_or_else(|| anyhow!("connection not found: {id}"))
    }

    fn create(&self, _config: &CreateRequest) -> Result<()> {
        Ok(())
    }

    fn remove(&self, _id: &str) -> Result<()> {
        Ok(())
    }

    fn import(&self, path: &str) -> Result<String> {
        Ok(PathBuf::from(path)
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned())
    }

    fn set_autostart(&self, id: &str, enabled: bool) -> Result<()> {
        let mut data = self.data.lock().unwrap();
        data.calls.push(ProviderCall::SetAutostart {
            id: id.to_string(),
            enabled,
        });
        let conn = data
            .connections
            .iter_mut()
            .find(|conn| conn.id == id)
            .ok_or_else(|| anyhow!("connection not found: {id}"))?;
        conn.autostart = enabled;
        Ok(())
    }

    fn config_fields(&self) -> Vec<FormField> {
        Vec::new()
    }
}

#[derive(Default)]
struct MockKillSwitchData {
    active: bool,
    calls: Vec<&'static str>,
}

struct MockKillSwitch {
    data: Arc<Mutex<MockKillSwitchData>>,
}

impl MockKillSwitch {
    fn new() -> Self {
        Self {
            data: Arc::new(Mutex::new(MockKillSwitchData::default())),
        }
    }

    fn calls(&self) -> Vec<&'static str> {
        self.data.lock().unwrap().calls.clone()
    }
}

impl KillSwitchController for MockKillSwitch {
    fn enable(&self) -> Result<()> {
        let mut data = self.data.lock().unwrap();
        data.calls.push("enable");
        data.active = true;
        Ok(())
    }

    fn disable(&self) -> Result<()> {
        let mut data = self.data.lock().unwrap();
        data.calls.push("disable");
        data.active = false;
        Ok(())
    }

    fn is_active(&self) -> bool {
        self.data.lock().unwrap().active
    }
}

struct TestHarness {
    _tempdir: TempDir,
    config_path: PathBuf,
    providers: HashMap<ProviderKind, Arc<MockProvider>>,
    killswitch: Arc<MockKillSwitch>,
    reader: tokio::io::Lines<BufReader<tokio::net::unix::OwnedReadHalf>>,
    writer: tokio::net::unix::OwnedWriteHalf,
    server_task: tokio::task::JoinHandle<Result<()>>,
}

impl TestHarness {
    async fn new() -> Result<Self> {
        let tempdir = tempfile::tempdir()?;
        let socket_path = tempdir.path().join("multivpn.sock");
        let config_path = tempdir.path().join("config.toml");

        let providers = HashMap::from([
            (
                ProviderKind::WireGuard,
                Arc::new(MockProvider::new(
                    ProviderKind::WireGuard,
                    "Mock WireGuard",
                    vec![mock_connection(
                        ProviderKind::WireGuard,
                        "wg0",
                        ConnectionStatus::Disconnected,
                        false,
                    )],
                )),
            ),
            (
                ProviderKind::OpenVpn,
                Arc::new(MockProvider::new(
                    ProviderKind::OpenVpn,
                    "Mock OpenVPN",
                    vec![mock_connection(
                        ProviderKind::OpenVpn,
                        "ovpn0",
                        ConnectionStatus::Connected,
                        false,
                    )],
                )),
            ),
            (
                ProviderKind::ProtonVpn,
                Arc::new(MockProvider::new(
                    ProviderKind::ProtonVpn,
                    "Mock ProtonVPN",
                    vec![mock_connection(
                        ProviderKind::ProtonVpn,
                        "proton-us",
                        ConnectionStatus::Connecting,
                        false,
                    )],
                )),
            ),
            (
                ProviderKind::Tailscale,
                Arc::new(MockProvider::new(
                    ProviderKind::Tailscale,
                    "Mock Tailscale",
                    vec![mock_connection(
                        ProviderKind::Tailscale,
                        "tailnet",
                        ConnectionStatus::Connected,
                        true,
                    )],
                )),
            ),
        ]);

        let killswitch = Arc::new(MockKillSwitch::new());
        let registry_providers: HashMap<ProviderKind, Arc<dyn VpnProvider>> = providers
            .iter()
            .map(|(kind, provider)| (*kind, provider.clone() as Arc<dyn VpnProvider>))
            .collect();
        let state = Arc::new(RwLock::new(
            DaemonState::with_dependencies(
                Config::default(),
                Arc::new(StaticProviderRegistry::new(registry_providers)),
                killswitch.clone() as Arc<dyn KillSwitchController>,
            )
            .with_config_path(config_path.clone()),
        ));

        let listener = UnixListener::bind(&socket_path)?;
        let server_state = state.clone();
        let server_task = tokio::spawn(async move {
            let (stream, _) = listener.accept().await?;
            handle_client(stream, server_state).await
        });

        let stream = UnixStream::connect(&socket_path).await?;
        let (reader, writer) = stream.into_split();

        Ok(Self {
            _tempdir: tempdir,
            config_path,
            providers,
            killswitch,
            reader: BufReader::new(reader).lines(),
            writer,
            server_task,
        })
    }

    async fn request(&mut self, req: Request) -> Result<Response> {
        let encoded = encode(&req)?;
        self.writer.write_all(encoded.as_bytes()).await?;
        let line = self
            .reader
            .next_line()
            .await?
            .ok_or_else(|| anyhow!("server closed connection"))?;
        decode_response(&line)
    }
}

impl Drop for TestHarness {
    fn drop(&mut self) {
        self.server_task.abort();
    }
}

fn mock_connection(
    provider: ProviderKind,
    id: &str,
    status: ConnectionStatus,
    autostart: bool,
) -> VpnConnection {
    VpnConnection {
        id: id.to_string(),
        provider,
        name: id.to_string(),
        status,
        autostart,
        details: serde_json::json!({ "id": id }),
    }
}

#[tokio::test]
async fn list_connections_returns_connections_from_all_providers() -> Result<()> {
    let mut harness = TestHarness::new().await?;

    let response = harness.request(Request::ListConnections).await?;

    match response {
        Response::Connections { items } => {
            assert_eq!(items.len(), 4);
            assert!(items.iter().any(|item| item.provider == ProviderKind::WireGuard && item.id == "wg0"));
            assert!(items.iter().any(|item| item.provider == ProviderKind::OpenVpn && item.id == "ovpn0"));
            assert!(items.iter().any(|item| item.provider == ProviderKind::ProtonVpn && item.id == "proton-us"));
            assert!(items.iter().any(|item| item.provider == ProviderKind::Tailscale && item.id == "tailnet"));
        }
        other => panic!("unexpected response: {other:?}"),
    }

    for provider in harness.providers.values() {
        assert_eq!(provider.calls(), vec![ProviderCall::ListConnections]);
    }

    Ok(())
}

#[tokio::test]
async fn connect_and_disconnect_return_ok_responses() -> Result<()> {
    let mut harness = TestHarness::new().await?;

    let connect = harness
        .request(Request::Connect {
            provider: ProviderKind::WireGuard,
            id: "wg0".into(),
        })
        .await?;
    assert!(matches!(connect, Response::Ok { .. }));

    let disconnect = harness
        .request(Request::Disconnect {
            provider: ProviderKind::WireGuard,
            id: "wg0".into(),
        })
        .await?;
    assert!(matches!(disconnect, Response::Ok { .. }));

    assert_eq!(
        harness.providers[&ProviderKind::WireGuard].calls(),
        vec![
            ProviderCall::Connect("wg0".into()),
            ProviderCall::Disconnect("wg0".into()),
        ]
    );

    Ok(())
}

#[tokio::test]
async fn kill_switch_roundtrip_updates_state_and_persisted_config() -> Result<()> {
    let mut harness = TestHarness::new().await?;

    let initial = harness.request(Request::KillSwitchStatus).await?;
    assert!(matches!(initial, Response::KillSwitch { active: false }));

    let enabled = harness.request(Request::KillSwitchEnable).await?;
    assert!(matches!(enabled, Response::KillSwitch { active: true }));
    assert!(Config::load_from(&harness.config_path)?.general.kill_switch);

    let after_enable = harness.request(Request::KillSwitchStatus).await?;
    assert!(matches!(after_enable, Response::KillSwitch { active: true }));

    let disabled = harness.request(Request::KillSwitchDisable).await?;
    assert!(matches!(disabled, Response::KillSwitch { active: false }));
    assert!(!Config::load_from(&harness.config_path)?.general.kill_switch);

    let after_disable = harness.request(Request::KillSwitchStatus).await?;
    assert!(matches!(after_disable, Response::KillSwitch { active: false }));
    assert_eq!(harness.killswitch.calls(), vec!["enable", "disable"]);

    Ok(())
}

#[tokio::test]
async fn list_providers_returns_all_four_providers() -> Result<()> {
    let mut harness = TestHarness::new().await?;

    let response = harness.request(Request::ListProviders).await?;

    match response {
        Response::Providers { items } => {
            assert_eq!(items.len(), 4);
            assert_eq!(
                items.iter().map(|item| item.kind).collect::<Vec<_>>(),
                ProviderKind::all()
            );
            assert!(items.iter().all(|item| item.available));
        }
        other => panic!("unexpected response: {other:?}"),
    }

    Ok(())
}

#[tokio::test]
async fn status_request_returns_current_status_and_details() -> Result<()> {
    let mut harness = TestHarness::new().await?;

    let response = harness
        .request(Request::Status {
            provider: ProviderKind::ProtonVpn,
            id: "proton-us".into(),
        })
        .await?;

    match response {
        Response::Status {
            provider,
            id,
            status,
            details,
        } => {
            assert_eq!(provider, ProviderKind::ProtonVpn);
            assert_eq!(id, "proton-us");
            assert_eq!(status, ConnectionStatus::Connecting);
            assert_eq!(details, "details for proton-us");
        }
        other => panic!("unexpected response: {other:?}"),
    }

    assert_eq!(
        harness.providers[&ProviderKind::ProtonVpn].calls(),
        vec![
            ProviderCall::Status("proton-us".into()),
            ProviderCall::StatusDetails("proton-us".into()),
        ]
    );

    Ok(())
}

#[tokio::test]
async fn connect_to_missing_connection_returns_error_response() -> Result<()> {
    let mut harness = TestHarness::new().await?;

    let response = harness
        .request(Request::Connect {
            provider: ProviderKind::WireGuard,
            id: "does-not-exist".into(),
        })
        .await?;

    match response {
        Response::Error { message } => assert!(message.contains("connection not found: does-not-exist")),
        other => panic!("unexpected response: {other:?}"),
    }

    Ok(())
}

#[tokio::test]
async fn set_autostart_roundtrip_updates_connection_state() -> Result<()> {
    let mut harness = TestHarness::new().await?;

    let enable = harness
        .request(Request::SetAutostart {
            provider: ProviderKind::WireGuard,
            id: "wg0".into(),
            enabled: true,
        })
        .await?;
    assert!(matches!(enable, Response::Ok { .. }));

    let after_enable = harness.request(Request::ListConnections).await?;
    match after_enable {
        Response::Connections { items } => {
            let connection = items
                .iter()
                .find(|item| item.provider == ProviderKind::WireGuard && item.id == "wg0")
                .unwrap();
            assert!(connection.autostart);
        }
        other => panic!("unexpected response: {other:?}"),
    }

    let disable = harness
        .request(Request::SetAutostart {
            provider: ProviderKind::WireGuard,
            id: "wg0".into(),
            enabled: false,
        })
        .await?;
    assert!(matches!(disable, Response::Ok { .. }));

    let after_disable = harness.request(Request::ListConnections).await?;
    match after_disable {
        Response::Connections { items } => {
            let connection = items
                .iter()
                .find(|item| item.provider == ProviderKind::WireGuard && item.id == "wg0")
                .unwrap();
            assert!(!connection.autostart);
        }
        other => panic!("unexpected response: {other:?}"),
    }

    assert_eq!(
        harness.providers[&ProviderKind::WireGuard].calls(),
        vec![
            ProviderCall::SetAutostart {
                id: "wg0".into(),
                enabled: true,
            },
            ProviderCall::ListConnections,
            ProviderCall::SetAutostart {
                id: "wg0".into(),
                enabled: false,
            },
            ProviderCall::ListConnections,
        ]
    );

    Ok(())
}
