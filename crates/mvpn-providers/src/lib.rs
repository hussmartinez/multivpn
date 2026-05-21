pub mod command;
pub mod openvpn;
pub mod protonvpn;
pub mod tailscale;
pub mod wireguard;

use mvpn_core::provider::VpnProvider;
use mvpn_core::types::ProviderKind;

pub fn create_provider(kind: ProviderKind) -> Box<dyn VpnProvider> {
    match kind {
        ProviderKind::WireGuard => Box::new(wireguard::WireGuardProvider::new()),
        ProviderKind::OpenVpn => Box::new(openvpn::OpenVpnProvider::new()),
        ProviderKind::ProtonVpn => Box::new(protonvpn::ProtonVpnProvider::new()),
        ProviderKind::Tailscale => Box::new(tailscale::TailscaleProvider::new()),
    }
}

pub fn all_providers() -> Vec<Box<dyn VpnProvider>> {
    ProviderKind::all()
        .iter()
        .map(|kind| create_provider(*kind))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_all_providers() {
        for kind in ProviderKind::all() {
            let p = create_provider(*kind);
            assert_eq!(p.kind(), *kind);
            assert!(!p.display_name().is_empty());
            assert!(!p.install_hint().is_empty());
        }
    }

    #[test]
    fn all_providers_returns_four() {
        assert_eq!(all_providers().len(), 4);
    }

    #[test]
    fn provider_kinds_match() {
        let providers = all_providers();
        let kinds: Vec<ProviderKind> = providers.iter().map(|p| p.kind()).collect();
        assert_eq!(kinds, ProviderKind::all());
    }

    #[test]
    fn wireguard_config_fields_not_empty() {
        let p = create_provider(ProviderKind::WireGuard);
        let fields = p.config_fields();
        assert!(!fields.is_empty());
        assert!(fields.iter().any(|f| f.key == "addresses"));
        assert!(fields.iter().any(|f| f.key == "peer_public_key"));
        assert!(fields.iter().any(|f| f.key == "autostart"));
    }

    #[test]
    fn openvpn_config_fields() {
        let p = create_provider(ProviderKind::OpenVpn);
        let fields = p.config_fields();
        assert!(fields.iter().any(|f| f.key == "config_path"));
    }

    #[test]
    fn protonvpn_config_fields() {
        let p = create_provider(ProviderKind::ProtonVpn);
        let fields = p.config_fields();
        assert!(fields.iter().any(|f| f.key == "server"));
    }

    #[test]
    fn tailscale_config_fields_empty() {
        let p = create_provider(ProviderKind::Tailscale);
        assert!(p.config_fields().is_empty());
    }

    #[test]
    fn protonvpn_create_not_supported() {
        let p = create_provider(ProviderKind::ProtonVpn);
        let req = mvpn_core::types::CreateRequest::default();
        assert!(p.create(&req).is_err());
    }

    #[test]
    fn tailscale_create_not_supported() {
        let p = create_provider(ProviderKind::Tailscale);
        let req = mvpn_core::types::CreateRequest::default();
        assert!(p.create(&req).is_err());
    }

    #[test]
    fn tailscale_import_not_supported() {
        let p = create_provider(ProviderKind::Tailscale);
        assert!(p.import("/some/path").is_err());
    }

    #[test]
    fn tailscale_remove_not_supported() {
        let p = create_provider(ProviderKind::Tailscale);
        assert!(p.remove("default").is_err());
    }

    #[test]
    fn openvpn_create_not_supported() {
        let p = create_provider(ProviderKind::OpenVpn);
        let req = mvpn_core::types::CreateRequest::default();
        assert!(p.create(&req).is_err());
    }
}
