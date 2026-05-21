mod client;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use mvpn_core::config::Config;
use mvpn_core::ipc::{Request, Response};
use mvpn_core::types::ProviderKind;
use std::fs;
use std::process::Command;

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
    /// Manage multivpn config
    Config {
        #[command(subcommand)]
        command: ConfigCommands,
    },
    /// List available providers
    Providers,
}

#[derive(Subcommand)]
enum ConfigCommands {
    /// Print the current config file contents
    Show,
    /// Print the config file path
    Path,
    /// Open the config file in $EDITOR
    Edit,
    /// Set a config value
    Set {
        key: String,
        value: String,
    },
    /// Get a config value
    Get {
        key: String,
    },
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

    match cli.command {
        Commands::Config { command } => handle_config_command(command),
        command => {
            let request = match command {
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
                Commands::Config { .. } => unreachable!(),
            };

            let response = client::send(&request)?;
            print_response(&response);
            Ok(())
        }
    }
}

fn print_response(response: &Response) {
    match response {
        Response::Ok { message } => println!("{message}"),
        Response::Error { message } => eprintln!("error: {message}"),
        Response::ConfigValue { value } => println!("{value}"),
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

fn handle_config_command(command: ConfigCommands) -> Result<()> {
    match command {
        ConfigCommands::Show => {
            let path = Config::config_path();
            if path.exists() {
                print!(
                    "{}",
                    fs::read_to_string(&path)
                        .with_context(|| format!("failed to read {}", path.display()))?
                );
            } else {
                print!("{}", toml::to_string_pretty(&Config::default())?);
            }
            Ok(())
        }
        ConfigCommands::Path => {
            println!("{}", Config::config_path().display());
            Ok(())
        }
        ConfigCommands::Edit => edit_config(),
        ConfigCommands::Set { key, value } => {
            let response = client::send(&Request::ConfigSet { key, value })?;
            print_response(&response);
            Ok(())
        }
        ConfigCommands::Get { key } => {
            let response = client::send(&Request::ConfigGet { key })?;
            print_response(&response);
            Ok(())
        }
    }
}

fn edit_config() -> Result<()> {
    let path = Config::config_path();
    if !path.exists() {
        Config::default().save()?;
    }

    let editor = std::env::var("EDITOR").context("$EDITOR is not set")?;
    let status = Command::new("sh")
        .arg("-c")
        .arg("$EDITOR \"$1\"")
        .arg("sh")
        .arg(&path)
        .status()
        .with_context(|| format!("failed to launch editor '{editor}'"))?;

    if !status.success() {
        bail!("editor exited with status {status}");
    }

    Ok(())
}
