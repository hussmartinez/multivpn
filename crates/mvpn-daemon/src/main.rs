use anyhow::Result;
use mvpn_core::config::Config;
use mvpn_core::ipc::SOCKET_PATH;
use mvpn_daemon::{autoconnect, killswitch, server, state::DaemonState};
use std::sync::Arc;
use tokio::net::UnixListener;
use tokio::sync::RwLock;

#[tokio::main]
async fn main() -> Result<()> {
    let config = Config::load().unwrap_or_default();
    let state = Arc::new(RwLock::new(DaemonState::new(config)));

    // Apply kill switch if it was persisted
    {
        let s = state.read().await;
        if s.config.general.kill_switch {
            if let Err(e) = killswitch::enable() {
                eprintln!("failed to restore kill switch: {e}");
            }
        }
    }

    // Autoconnect configured VPNs
    {
        let s = state.read().await;
        autoconnect::run(&s.config);
    }

    let _ = std::fs::remove_file(SOCKET_PATH);
    let listener = UnixListener::bind(SOCKET_PATH).map_err(|e| {
        if e.kind() == std::io::ErrorKind::AddrInUse {
            anyhow::anyhow!(
                "another mvpn-daemon instance may already be running (socket {SOCKET_PATH} in use)"
            )
        } else {
            anyhow::anyhow!("failed to bind socket {SOCKET_PATH}: {e}")
        }
    })?;

    // Allow non-root clients to connect
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(SOCKET_PATH, std::fs::Permissions::from_mode(0o666))?;
    }

    eprintln!("mvpn-daemon listening on {SOCKET_PATH}");

    loop {
        let (stream, _) = listener.accept().await?;
        let state = Arc::clone(&state);
        tokio::spawn(async move {
            if let Err(e) = server::handle_client(stream, state).await {
                eprintln!("client error: {e}");
            }
        });
    }
}
