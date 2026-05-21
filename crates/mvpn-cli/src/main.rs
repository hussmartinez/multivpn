mod client;

use anyhow::{Result, bail};
use clap::{Args, Parser, Subcommand};
use mvpn_core::ipc::{Request, Response};
use mvpn_core::types::{ConnectionStatus, ProviderKind};
use mvpn_providers::create_provider;
use std::process::Command;

#[derive(Parser, Debug)]
#[command(name = "mvpn", about = "MultiVPN manager CLI", version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// List all VPN connections across all providers
    List(JsonFlag),
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
    /// Manage autoconnect entries
    Autoconnect {
        #[command(subcommand)]
        command: AutoconnectCommands,
    },
    /// Show install instructions for a provider and optionally run them
    Install {
        /// Provider
        provider: String,
        /// Run the detected install command when unambiguous
        #[arg(long)]
        run: bool,
    },
    /// Manage the kill switch
    Killswitch {
        /// on, off, or status
        action: String,
    },
    /// List available providers
    Providers(JsonFlag),
}

#[derive(Args, Debug, Clone, Copy, Default)]
struct JsonFlag {
    /// Print machine-readable JSON
    #[arg(long)]
    json: bool,
}

#[derive(Subcommand, Debug)]
enum AutoconnectCommands {
    /// Show configured autoconnect entries
    List,
    /// Add an autoconnect entry
    Add {
        /// Provider
        provider: String,
        /// Connection ID
        id: String,
    },
    /// Remove an autoconnect entry
    Remove {
        /// Provider
        provider: String,
        /// Connection ID
        id: String,
    },
}

enum Action {
    Daemon { request: Request, json: bool },
    Install { provider: ProviderKind, run: bool },
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

fn build_action(cli: Cli) -> Result<Action> {
    let action = match cli.command {
        Commands::List(flags) => Action::Daemon {
            request: Request::ListConnections,
            json: flags.json,
        },
        Commands::Connect { provider, id } => Action::Daemon {
            request: Request::Connect {
                provider: parse_provider(&provider)?,
                id,
            },
            json: false,
        },
        Commands::Disconnect { provider, id } => Action::Daemon {
            request: Request::Disconnect {
                provider: parse_provider(&provider)?,
                id,
            },
            json: false,
        },
        Commands::Status { provider, id } => Action::Daemon {
            request: Request::Status {
                provider: parse_provider(&provider)?,
                id,
            },
            json: false,
        },
        Commands::Import { provider, path } => Action::Daemon {
            request: Request::Import {
                provider: parse_provider(&provider)?,
                path,
            },
            json: false,
        },
        Commands::Remove { provider, id } => Action::Daemon {
            request: Request::Remove {
                provider: parse_provider(&provider)?,
                id,
            },
            json: false,
        },
        Commands::Autoconnect { command } => Action::Daemon {
            request: match command {
                AutoconnectCommands::List => Request::AutoconnectList,
                AutoconnectCommands::Add { provider, id } => Request::AutoconnectAdd {
                    provider: parse_provider(&provider)?,
                    id,
                },
                AutoconnectCommands::Remove { provider, id } => Request::AutoconnectRemove {
                    provider: parse_provider(&provider)?,
                    id,
                },
            },
            json: false,
        },
        Commands::Install { provider, run } => Action::Install {
            provider: parse_provider(&provider)?,
            run,
        },
        Commands::Killswitch { action } => Action::Daemon {
            request: match action.as_str() {
                "on" | "enable" => Request::KillSwitchEnable,
                "off" | "disable" => Request::KillSwitchDisable,
                "status" => Request::KillSwitchStatus,
                _ => bail!("killswitch action must be: on, off, or status"),
            },
            json: false,
        },
        Commands::Providers(flags) => Action::Daemon {
            request: Request::ListProviders,
            json: flags.json,
        },
    };

    Ok(action)
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match build_action(cli)? {
        Action::Daemon { request, json } => {
            let response = client::send(&request)?;
            print_response(&response, json)?;
        }
        Action::Install { provider, run } => {
            run_install(provider, run)?;
        }
    }

    Ok(())
}

fn run_install(provider: ProviderKind, run: bool) -> Result<()> {
    let provider_impl = create_provider(provider);
    let hint = provider_impl.install_hint();

    println!("{} install hint:", provider_impl.display_name());
    println!("{hint}");

    if !run {
        return Ok(());
    }

    let command = install_command_from_hint(hint).ok_or_else(|| {
        anyhow::anyhow!(
            "cannot auto-run install instructions for {provider}; run the shown command manually"
        )
    })?;

    println!("running: {command}");
    let status = Command::new("sh").arg("-lc").arg(command).status()?;
    if !status.success() {
        bail!("install command exited with status {status}");
    }

    Ok(())
}

fn install_command_from_hint(hint: &str) -> Option<&str> {
    if hint.contains(" or ") {
        return None;
    }

    let command = hint.split(" (").next()?.trim();
    if command.is_empty() {
        return None;
    }
    Some(command)
}

fn print_response(response: &Response, json: bool) -> Result<()> {
    match response {
        Response::Ok { message } => println!("{message}"),
        Response::Error { message } => eprintln!("error: {message}"),
        Response::Connections { items } => {
            if json {
                println!("{}", serde_json::to_string_pretty(items)?);
                return Ok(());
            }

            if items.is_empty() {
                println!("No connections found.");
                return Ok(());
            }
            println!(
                "{:<12} {:<16} {:<14} {:<10}",
                "PROVIDER", "NAME", "STATUS", "AUTOSTART"
            );
            for conn in items {
                let status = match &conn.status {
                    ConnectionStatus::Connected => "connected",
                    ConnectionStatus::Disconnected => "disconnected",
                    ConnectionStatus::Connecting => "connecting",
                    ConnectionStatus::Error(e) => e.as_str(),
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
            println!(
                "Kill switch: {}",
                if *active { "ACTIVE" } else { "inactive" }
            );
        }
        Response::AutoconnectEntries { items } => {
            if items.is_empty() {
                println!("No autoconnect entries configured.");
                return Ok(());
            }
            println!("{:<12} {}", "PROVIDER", "ID");
            for entry in items {
                println!("{:<12} {}", entry.provider, entry.id);
            }
        }
        Response::Providers { items } => {
            if json {
                println!("{}", serde_json::to_string_pretty(items)?);
                return Ok(());
            }

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

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_cli(args: &[&str]) -> Cli {
        Cli::try_parse_from(args).unwrap()
    }

    #[test]
    fn parse_provider_aliases() {
        assert_eq!(parse_provider("wg").unwrap(), ProviderKind::WireGuard);
        assert_eq!(parse_provider("ovpn").unwrap(), ProviderKind::OpenVpn);
        assert_eq!(parse_provider("proton").unwrap(), ProviderKind::ProtonVpn);
        assert_eq!(parse_provider("ts").unwrap(), ProviderKind::Tailscale);
    }

    #[test]
    fn parse_provider_is_case_insensitive() {
        assert_eq!(
            parse_provider("WireGuard").unwrap(),
            ProviderKind::WireGuard
        );
        assert_eq!(
            parse_provider("TAILSCALE").unwrap(),
            ProviderKind::Tailscale
        );
    }

    #[test]
    fn parse_provider_rejects_unknown_provider() {
        let err = parse_provider("nope").unwrap_err().to_string();
        assert!(err.contains("unknown provider: nope"));
    }

    #[test]
    fn parse_list_json_flag() {
        let cli = parse_cli(&["mvpn", "list", "--json"]);
        match cli.command {
            Commands::List(flags) => assert!(flags.json),
            _ => panic!("wrong command"),
        }
    }

    #[test]
    fn parse_providers_json_flag() {
        let cli = parse_cli(&["mvpn", "providers", "--json"]);
        match cli.command {
            Commands::Providers(flags) => assert!(flags.json),
            _ => panic!("wrong command"),
        }
    }

    #[test]
    fn parse_autoconnect_add_command() {
        let cli = parse_cli(&["mvpn", "autoconnect", "add", "wg", "work"]);
        match build_action(cli).unwrap() {
            Action::Daemon {
                request: Request::AutoconnectAdd { provider, id },
                ..
            } => {
                assert_eq!(provider, ProviderKind::WireGuard);
                assert_eq!(id, "work");
            }
            _ => panic!("wrong action"),
        }
    }

    #[test]
    fn parse_install_run_flag() {
        let cli = parse_cli(&["mvpn", "install", "tailscale", "--run"]);
        match build_action(cli).unwrap() {
            Action::Install { provider, run } => {
                assert_eq!(provider, ProviderKind::Tailscale);
                assert!(run);
            }
            _ => panic!("wrong action"),
        }
    }

    #[test]
    fn parse_killswitch_action() {
        let cli = parse_cli(&["mvpn", "killswitch", "status"]);
        match build_action(cli).unwrap() {
            Action::Daemon {
                request: Request::KillSwitchStatus,
                ..
            } => {}
            _ => panic!("wrong action"),
        }
    }

    #[test]
    fn install_command_requires_unambiguous_hint() {
        assert_eq!(
            install_command_from_hint(
                "sudo apt install wireguard-tools (Debian/Ubuntu) or sudo pacman -S wireguard-tools (Arch)"
            ),
            None
        );
        assert_eq!(
            install_command_from_hint("curl -fsSL https://tailscale.com/install.sh | sh"),
            Some("curl -fsSL https://tailscale.com/install.sh | sh")
        );
        assert_eq!(
            install_command_from_hint("sudo apt install openvpn"),
            Some("sudo apt install openvpn")
        );
    }
}
