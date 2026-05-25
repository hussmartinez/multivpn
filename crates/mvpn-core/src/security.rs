use anyhow::{Result, bail};
use std::path::Path;

pub fn validate_import_path(path: &str) -> Result<&Path> {
    let path = path.trim();
    if path.is_empty() {
        bail!("import path is empty");
    }

    let p = Path::new(path);

    if !p.is_absolute() {
        bail!("import path must be absolute");
    }

    for component in p.components() {
        if let std::path::Component::ParentDir = component {
            bail!("path traversal (..) is not allowed in import paths");
        }
    }

    if p.is_symlink() {
        bail!("symlinks are not allowed as import paths");
    }

    Ok(p)
}

pub fn validate_connection_id(id: &str) -> Result<()> {
    if id.is_empty() {
        bail!("connection ID cannot be empty");
    }
    if id.len() > 255 {
        bail!("connection ID too long (max 255 characters)");
    }
    if !id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
    {
        bail!("connection ID contains invalid characters (allowed: alphanumeric, _, -, .)");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn import_path_rejects_empty() {
        assert!(validate_import_path("").is_err());
        assert!(validate_import_path("  ").is_err());
    }

    #[test]
    fn import_path_rejects_relative() {
        assert!(validate_import_path("configs/wg0.conf").is_err());
        assert!(validate_import_path("./wg0.conf").is_err());
    }

    #[test]
    fn import_path_rejects_traversal() {
        assert!(validate_import_path("/etc/wireguard/../shadow").is_err());
        assert!(validate_import_path("/tmp/../../etc/passwd").is_err());
    }

    #[test]
    fn import_path_accepts_valid_absolute() {
        assert!(validate_import_path("/home/user/wg0.conf").is_ok());
        assert!(validate_import_path("/tmp/my-vpn.ovpn").is_ok());
    }

    #[test]
    fn connection_id_rejects_empty() {
        assert!(validate_connection_id("").is_err());
    }

    #[test]
    fn connection_id_rejects_special_chars() {
        assert!(validate_connection_id("wg0; rm -rf /").is_err());
        assert!(validate_connection_id("../etc").is_err());
        assert!(validate_connection_id("name with spaces").is_err());
    }

    #[test]
    fn connection_id_accepts_valid() {
        assert!(validate_connection_id("wg0").is_ok());
        assert!(validate_connection_id("my-vpn_config.1").is_ok());
        assert!(validate_connection_id("office-vpn").is_ok());
    }

    #[test]
    fn connection_id_rejects_too_long() {
        let long = "a".repeat(256);
        assert!(validate_connection_id(&long).is_err());
    }
}
