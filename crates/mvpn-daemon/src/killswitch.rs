use anyhow::{Result, bail};
use mvpn_providers::command::{command_exists, run};

const NFT_TABLE: &str = "multivpn_killswitch";

const VPN_INTERFACES: &[&str] = &["wg*", "tailscale*", "tun*", "proton*"];

pub fn enable() -> Result<()> {
    if command_exists("nft") {
        enable_nftables()
    } else if command_exists("iptables") {
        enable_iptables()
    } else {
        bail!("neither nft nor iptables found")
    }
}

pub fn disable() -> Result<()> {
    if command_exists("nft") {
        disable_nftables()
    } else if command_exists("iptables") {
        disable_iptables()
    } else {
        bail!("neither nft nor iptables found")
    }
}

pub fn is_active() -> bool {
    if command_exists("nft") {
        run("nft", &["list", "table", "inet", NFT_TABLE], true).is_ok()
    } else if command_exists("iptables") {
        run("iptables", &["-L", "MULTIVPN_KILLSWITCH"], true).is_ok()
    } else {
        false
    }
}

fn enable_nftables() -> Result<()> {
    let _ = disable_nftables();

    let mut ruleset = format!(
        "table inet {NFT_TABLE} {{\n  chain output {{\n    type filter hook output priority 0; policy drop;\n"
    );

    // Allow loopback
    ruleset.push_str("    oifname \"lo\" accept\n");

    // Allow established/related
    ruleset.push_str("    ct state established,related accept\n");

    // Allow DHCP
    ruleset.push_str("    udp dport { 67, 68 } accept\n");

    // Allow DNS (needed for VPN resolution)
    ruleset.push_str("    udp dport 53 accept\n");
    ruleset.push_str("    tcp dport 53 accept\n");

    // Allow VPN interfaces
    for iface in VPN_INTERFACES {
        ruleset.push_str(&format!("    oifname \"{iface}\" accept\n"));
    }

    ruleset.push_str("  }\n}\n");

    run("nft", &["-f", "-"], true)?;
    // Actually pass the ruleset via stdin
    mvpn_providers::command::run_with_stdin("nft", &["-f", "-"], true, Some(ruleset.as_bytes()))?;

    Ok(())
}

fn disable_nftables() -> Result<()> {
    let _ = run("nft", &["delete", "table", "inet", NFT_TABLE], true);
    Ok(())
}

fn enable_iptables() -> Result<()> {
    let _ = disable_iptables();

    // Create custom chain
    run("iptables", &["-N", "MULTIVPN_KILLSWITCH"], true)?;

    // Allow loopback
    run(
        "iptables",
        &["-A", "MULTIVPN_KILLSWITCH", "-o", "lo", "-j", "ACCEPT"],
        true,
    )?;

    // Allow established/related
    run(
        "iptables",
        &[
            "-A",
            "MULTIVPN_KILLSWITCH",
            "-m",
            "conntrack",
            "--ctstate",
            "ESTABLISHED,RELATED",
            "-j",
            "ACCEPT",
        ],
        true,
    )?;

    // Allow DHCP
    run(
        "iptables",
        &[
            "-A",
            "MULTIVPN_KILLSWITCH",
            "-p",
            "udp",
            "--dport",
            "67:68",
            "-j",
            "ACCEPT",
        ],
        true,
    )?;

    // Allow DNS
    run(
        "iptables",
        &[
            "-A",
            "MULTIVPN_KILLSWITCH",
            "-p",
            "udp",
            "--dport",
            "53",
            "-j",
            "ACCEPT",
        ],
        true,
    )?;
    run(
        "iptables",
        &[
            "-A",
            "MULTIVPN_KILLSWITCH",
            "-p",
            "tcp",
            "--dport",
            "53",
            "-j",
            "ACCEPT",
        ],
        true,
    )?;

    // Allow VPN interfaces
    for iface in VPN_INTERFACES {
        run(
            "iptables",
            &["-A", "MULTIVPN_KILLSWITCH", "-o", iface, "-j", "ACCEPT"],
            true,
        )?;
    }

    // Drop everything else
    run(
        "iptables",
        &["-A", "MULTIVPN_KILLSWITCH", "-j", "DROP"],
        true,
    )?;

    // Insert into OUTPUT chain
    run(
        "iptables",
        &["-I", "OUTPUT", "-j", "MULTIVPN_KILLSWITCH"],
        true,
    )?;

    Ok(())
}

fn disable_iptables() -> Result<()> {
    let _ = run(
        "iptables",
        &["-D", "OUTPUT", "-j", "MULTIVPN_KILLSWITCH"],
        true,
    );
    let _ = run("iptables", &["-F", "MULTIVPN_KILLSWITCH"], true);
    let _ = run("iptables", &["-X", "MULTIVPN_KILLSWITCH"], true);
    Ok(())
}
