# multivpn

A modular multi-VPN manager for Linux. Manage WireGuard, OpenVPN, ProtonVPN, and Tailscale connections from a single interface.

## Features

- Multiple simultaneous VPN connections
- Global kill switch (nftables/iptables)
- Auto-connect on boot via daemon
- CLI, TUI, and system tray interfaces
- Plugin-like provider architecture

## Components

| Binary | Description |
|--------|-------------|
| `mvpn` | CLI client |
| `mvpn-daemon` | systemd daemon (kill switch, autoconnect) |
| `mvpn-tui` | Terminal UI (ratatui) |
| `mvpn-tray` | System tray icon (ksni) |

## Installation

### Build from source

```sh
git clone https://github.com/hussmartinez/multivpn.git
cd multivpn
sudo make install    # builds release + installs binaries + systemd unit
sudo make enable     # start and enable the daemon
```

Requires Rust toolchain ([rustup.rs](https://rustup.rs)).

### Manual

```sh
git clone https://github.com/hussmartinez/multivpn.git
cd multivpn
cargo build --release
sudo cp target/release/{mvpn,mvpn-daemon,mvpn-tui,mvpn-tray} /usr/local/bin/
sudo cp crates/mvpn-daemon/multivpn.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now multivpn
```

## Usage

```sh
mvpn list                          # list all connections
mvpn connect wireguard wg0         # connect
mvpn disconnect wireguard wg0      # disconnect
mvpn killswitch on                 # enable kill switch
mvpn providers                     # show available providers
mvpn install tailscale --run       # show/run install command
mvpn autoconnect add wg wg0        # auto-connect on boot
mvpn config show                   # show config
mvpn-tui                           # launch TUI
```

## Configuration

Config lives at `~/.config/multivpn/config.toml`:

```toml
[general]
kill_switch = false

[autoconnect]
connections = [
    { provider = "wireguard", id = "wg0" },
]

[providers.wireguard]
config_dir = "/etc/wireguard"
```

## Uninstall

```sh
make disable    # stop and disable daemon
make uninstall  # remove binaries and systemd unit
```

## License

MIT
