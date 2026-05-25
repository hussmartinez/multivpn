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
mvpn update                        # pull + rebuild + reinstall
mvpn --version                     # show version
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

## Security

The daemon runs as root and enforces:

- **Socket authentication**: only root or members of the `multivpn` group can send commands (SO_PEERCRED)
- **Input validation**: connection IDs restricted to `[a-zA-Z0-9_.-]`, import paths must be absolute with no `..` traversal or symlinks
- **No shell injection**: all command execution uses argument arrays, not shell interpolation
- **Atomic file permissions**: config files created with mode 600 from the start
- **Output sanitization**: all daemon responses stripped of control characters before display
- **Request size limit**: 64KB max per IPC request

To allow a non-root user to use multivpn:

```sh
sudo groupadd multivpn
sudo usermod -aG multivpn $USER
# log out and back in
```

## Updating

```sh
mvpn update        # pull latest, rebuild, reinstall, restart daemon
mvpn update -y     # skip confirmation
```

## Uninstall

```sh
make disable    # stop and disable daemon
make uninstall  # remove binaries and systemd unit
```

## License

MIT
