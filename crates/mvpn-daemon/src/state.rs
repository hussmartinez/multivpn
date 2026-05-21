use mvpn_core::config::Config;
use mvpn_core::provider::VpnProvider;
use mvpn_core::types::ProviderKind;
use mvpn_providers::create_provider;

pub struct DaemonState {
    pub config: Config,
    pub kill_switch_active: bool,
}

impl DaemonState {
    pub fn new(config: Config) -> Self {
        let kill_switch_active = config.general.kill_switch;
        Self {
            config,
            kill_switch_active,
        }
    }

    pub fn provider(&self, kind: ProviderKind) -> Box<dyn VpnProvider> {
        create_provider(kind)
    }
}
