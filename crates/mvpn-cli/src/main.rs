mod client;

use anyhow::{Result, bail};
use clap::{Parser, Subcommand};
use mvpn_core::ipc::{Request, Response};
use mvpn_core::types::ProviderKind;

#[derive(Parser)]
#[command(name = "mvpn", about = "MultiVPN manager CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// List all VPN connections across all providers
    List,
    /// Connect a VPN
    Connect {
        /// Provider: wireguard, openvpn, protonvpn, tailscale
        provider: String,
        /// Connection ID
        id: String,
    },
    /// Disconnect a VPN
    Disconnect {
        /// Provider
        provider: String,
        /// Connection ID
        id: String,
    },
    /// Show status of a connection
    Status {
        /// Provider
        provider: String,
        /// Connection ID
        id: String,
    },
    /// Import a config file
    Import {
        /// Provider
        provider: String,
        /// Path to config file
        path: String,
    },
    /// Remove a VPN connection
    Remove {
        /// Provider
        provider: String,
        /// Connection ID
        id: String,
    },
    /// Manage the kill switch
    Killswitch {
        /// on, off, or status
        action: String,
    },
    /// List available providers
    Providers,
}

fn parse_provider(s: &str) -> Result<ProviderKind> {
    match s.to_lowercase().as_str() {
        "wireguard" | "wg" => Ok(ProviderKind::WireGuard),
        "openvpn" | "ovpn" => Ok(ProviderKind::OpenVpn),
        "protonvpn" | "proton" => Ok(ProviderKind::ProtonVpn),
        "tailscale" | "ts" => Ok(ProviderKind::Tailscale),
        _ => bail!("unknown provider: {s}. Use: wireguard, openvpn, protonvpn, tailscale"),
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let request = match cli.command {
        Commands::List => Request::ListConnections,
        Commands::Connect { provider, id } => Request::Connect {
            provider: parse_provider(&provider)?,
            id,
        },
        Commands::Disconnect { provider, id } => Request::Disconnect {
            provider: parse_provider(&provider)?,
            id,
        },
        Commands::Status { provider, id } => Request::Status {
            provider: parse_provider(&provider)?,
            id,
        },
        Commands::Import { provider, path } => Request::Import {
            provider: parse_provider(&provider)?,
            path,
        },
        Commands::Remove { provider, id } => Request::Remove {
            provider: parse_provider(&provider)?,
            id,
        },
        Commands::Killswitch { action } => match action.as_str() {
            "on" | "enable" => Request::KillSwitchEnable,
            "off" | "disable" => Request::KillSwitchDisable,
            "status" => Request::KillSwitchStatus,
            _ => bail!("killswitch action must be: on, off, or status"),
        },
        Commands::Providers => Request::ListProviders,
    };

    let response = client::send(&request)?;
    print_response(&response);
    Ok(())
}

fn print_response(response: &Response) {
    match response {
        Response::Ok { message } => println!("{message}"),
        Response::Error { message } => eprintln!("error: {message}"),
        Response::Connections { items } => {
            if items.is_empty() {
                println!("No connections found.");
                return;
            }
            println!("{:<12} {:<16} {:<14} {:<10}", "PROVIDER", "NAME", "STATUS", "AUTOSTART");
            for conn in items {
                let status = match &conn.status {
                    mvpn_core::types::ConnectionStatus::Connected => "connected",
                    mvpn_core::types::ConnectionStatus::Disconnected => "disconnected",
                    mvpn_core::types::ConnectionStatus::Connecting => "connecting",
                    mvpn_core::types::ConnectionStatus::Error(e) => e.as_str(),
                };
                println!(
                    "{:<12} {:<16} {:<14} {:<10}",
                    conn.provider,
                    conn.name,
                    status,
                    if conn.autostart { "yes" } else { "no" }
                );
            }
        }
        Response::Status {
            provider,
            id,
            status,
            details,
        } => {
            println!("{provider} {id}: {status:?}");
            if !details.is_empty() {
                println!("{details}");
            }
        }
        Response::KillSwitch { active } => {
            println!("Kill switch: {}", if *active { "ACTIVE" } else { "inactive" });
        }
        Response::Providers { items } => {
            println!("{:<12} {:<12} {}", "PROVIDER", "STATUS", "INSTALL HINT");
            for p in items {
                println!(
                    "{:<12} {:<12} {}",
                    p.display_name,
                    if p.available { "available" } else { "missing" },
                    if p.available { "-" } else { &p.install_hint }
                );
            }
        }
    }
}
