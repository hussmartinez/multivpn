use anyhow::{Context, Result, bail};
use ksni::{
    Status, ToolTip,
    blocking::{Handle, TrayMethods},
    Tray,
    menu::{CheckmarkItem, MenuItem, StandardItem},
};
use mvpn_core::{
    ipc::{self, Request, Response, SOCKET_PATH},
    types::{ConnectionStatus, ProviderKind, VpnConnection},
};
use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixStream,
    process,
    thread,
    time::Duration,
};

const REFRESH_INTERVAL: Duration = Duration::from_secs(5);

fn main() -> Result<()> {
    let handle = MvpnTray::new()
        .assume_sni_available(true)
        .spawn()
        .context("failed to start tray service")?;
    refresh_tray(&handle);
    spawn_refresh_loop(handle.clone());

    loop {
        thread::park();
    }
}

fn spawn_refresh_loop(handle: Handle<MvpnTray>) {
    thread::spawn(move || loop {
        thread::sleep(REFRESH_INTERVAL);
        refresh_tray(&handle);
    });
}

fn refresh_tray(handle: &Handle<MvpnTray>) {
    let _ = match fetch_state() {
        Ok(state) => handle.update(|tray| tray.apply_state(state)),
        Err(error) => handle.update(|tray| tray.set_error(error.to_string())),
    };
}

fn fetch_state() -> Result<TrayState> {
    let connections = match send(&Request::ListConnections)? {
        Response::Connections { items } => items,
        Response::Error { message } => bail!(message),
        other => bail!("unexpected response for list_connections: {other:?}"),
    };

    let kill_switch_active = match send(&Request::KillSwitchStatus)? {
        Response::KillSwitch { active } => active,
        Response::Error { message } => bail!(message),
        other => bail!("unexpected response for kill_switch_status: {other:?}"),
    };

    Ok(TrayState {
        connections,
        kill_switch_active,
        last_error: None,
    })
}

fn send(request: &Request) -> Result<Response> {
    let mut stream =
        UnixStream::connect(SOCKET_PATH).context("cannot connect to mvpn-daemon; is it running?")?;

    let encoded = ipc::encode(request)?;
    stream.write_all(encoded.as_bytes())?;
    stream.shutdown(std::net::Shutdown::Write)?;

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line)?;

    ipc::decode_response(&line)
}

#[derive(Clone, Debug, Default)]
struct TrayState {
    connections: Vec<VpnConnection>,
    kill_switch_active: bool,
    last_error: Option<String>,
}

#[derive(Clone, Debug, Default)]
struct MvpnTray {
    state: TrayState,
}

impl MvpnTray {
    fn new() -> Self {
        Self::default()
    }

    fn apply_state(&mut self, state: TrayState) {
        self.state = state;
    }

    fn set_error(&mut self, error: String) {
        self.state.last_error = Some(error);
    }

    fn summary(&self) -> String {
        let connected = self
            .state
            .connections
            .iter()
            .filter(|connection| matches!(connection.status, ConnectionStatus::Connected))
            .count();

        let total = self.state.connections.len();
        let base = if connected > 0 {
            format!("Connected ({connected}/{total})")
        } else {
            "Disconnected".to_string()
        };

        if self.state.kill_switch_active {
            format!("{base} | kill switch on")
        } else {
            format!("{base} | kill switch off")
        }
    }

    fn icon_name_for_status(&self) -> String {
        if self
            .state
            .connections
            .iter()
            .any(|connection| matches!(connection.status, ConnectionStatus::Connected))
        {
            "network-vpn-symbolic".to_string()
        } else {
            "network-offline-symbolic".to_string()
        }
    }
}

impl Tray for MvpnTray {
    fn id(&self) -> String {
        "multivpn".to_string()
    }

    fn title(&self) -> String {
        self.summary()
    }

    fn status(&self) -> Status {
        if self.state.last_error.is_some() {
            Status::NeedsAttention
        } else if self
            .state
            .connections
            .iter()
            .any(|connection| matches!(connection.status, ConnectionStatus::Connected))
        {
            Status::Active
        } else {
            Status::Passive
        }
    }

    fn tool_tip(&self) -> ToolTip {
        ToolTip {
            title: "multivpn".into(),
            description: match &self.state.last_error {
                Some(error) => format!("{}\n{error}", self.summary()),
                None => self.summary(),
            },
            ..Default::default()
        }
    }

    fn icon_name(&self) -> String {
        self.icon_name_for_status()
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let mut items = vec![
            MenuItem::Standard(StandardItem {
                label: self.summary(),
                enabled: false,
                ..Default::default()
            }),
            MenuItem::Checkmark(CheckmarkItem {
                label: "Kill switch".into(),
                checked: self.state.kill_switch_active,
                activate: Box::new(|tray: &mut Self| tray.toggle_kill_switch()),
                ..Default::default()
            }),
            MenuItem::Separator,
        ];

        if self.state.connections.is_empty() {
            items.push(MenuItem::Standard(StandardItem {
                label: "No connections".into(),
                enabled: false,
                ..Default::default()
            }));
        } else {
            for connection in &self.state.connections {
                let provider = connection.provider;
                let id = connection.id.clone();
                let label = format!(
                    "{} ({}) [{}]",
                    connection.name,
                    provider_label(provider),
                    status_label(&connection.status)
                );
                let connected = matches!(connection.status, ConnectionStatus::Connected);

                items.push(MenuItem::Checkmark(CheckmarkItem {
                    label,
                    checked: connected,
                    activate: Box::new(move |tray: &mut Self| {
                        tray.toggle_connection(provider, id.clone())
                    }),
                    ..Default::default()
                }));
            }
        }

        items.push(MenuItem::Separator);
        items.push(MenuItem::Standard(StandardItem {
            label: "Refresh".into(),
            activate: Box::new(|tray: &mut Self| tray.refresh_now()),
            ..Default::default()
        }));
        items.push(MenuItem::Standard(StandardItem {
            label: "Quit".into(),
            activate: Box::new(|_| process::exit(0)),
            ..Default::default()
        }));

        if let Some(error) = &self.state.last_error {
            items.insert(
                1,
                MenuItem::Standard(StandardItem {
                    label: format!("Error: {error}"),
                    enabled: false,
                    ..Default::default()
                }),
            );
        }

        items
    }
}

impl MvpnTray {
    fn refresh_now(&mut self) {
        match fetch_state() {
            Ok(state) => self.apply_state(state),
            Err(error) => self.set_error(error.to_string()),
        }
    }

    fn toggle_kill_switch(&mut self) {
        let request = if self.state.kill_switch_active {
            Request::KillSwitchDisable
        } else {
            Request::KillSwitchEnable
        };

        if let Err(error) = send_action(&request) {
            self.set_error(error.to_string());
            return;
        }

        self.refresh_now();
    }

    fn toggle_connection(&mut self, provider: ProviderKind, id: String) {
        let Some(connection) = self
            .state
            .connections
            .iter()
            .find(|connection| connection.provider == provider && connection.id == id)
        else {
            self.set_error(format!("connection not found: {provider}/{id}"));
            return;
        };

        let request = match connection.status {
            ConnectionStatus::Connected => Request::Disconnect { provider, id },
            _ => Request::Connect { provider, id },
        };

        if let Err(error) = send_action(&request) {
            self.set_error(error.to_string());
            return;
        }

        self.refresh_now();
    }
}

fn send_action(request: &Request) -> Result<()> {
    match send(request)? {
        Response::Ok { .. } => Ok(()),
        Response::Error { message } => bail!(message),
        other => bail!("unexpected response for action request: {other:?}"),
    }
}

fn provider_label(provider: ProviderKind) -> &'static str {
    match provider {
        ProviderKind::WireGuard => "WireGuard",
        ProviderKind::OpenVpn => "OpenVPN",
        ProviderKind::ProtonVpn => "ProtonVPN",
        ProviderKind::Tailscale => "Tailscale",
    }
}

fn status_label(status: &ConnectionStatus) -> &str {
    match status {
        ConnectionStatus::Connected => "connected",
        ConnectionStatus::Disconnected => "disconnected",
        ConnectionStatus::Connecting => "connecting",
        ConnectionStatus::Error(_) => "error",
    }
}
