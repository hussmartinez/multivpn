use mvpn_core::config::Config;
use mvpn_core::provider::VpnProvider;
use mvpn_core::types::ProviderKind;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use crate::killswitch::{self, KillSwitchController};

pub trait ProviderRegistry: Send + Sync {
    fn provider(&self, kind: ProviderKind) -> Arc<dyn VpnProvider>;
    fn all_providers(&self) -> Vec<Arc<dyn VpnProvider>>;
}

pub struct DefaultProviderRegistry;

impl ProviderRegistry for DefaultProviderRegistry {
    fn provider(&self, kind: ProviderKind) -> Arc<dyn VpnProvider> {
        Arc::from(mvpn_providers::create_provider(kind))
    }

    fn all_providers(&self) -> Vec<Arc<dyn VpnProvider>> {
        ProviderKind::all()
            .iter()
            .map(|kind| self.provider(*kind))
            .collect()
    }
}

pub struct DaemonState {
    pub config: Config,
    pub kill_switch_active: bool,
    provider_registry: Arc<dyn ProviderRegistry>,
    kill_switch_controller: Arc<dyn KillSwitchController>,
    config_path: Option<PathBuf>,
}

impl DaemonState {
    pub fn new(config: Config) -> Self {
        Self::with_dependencies(
            config,
            Arc::new(DefaultProviderRegistry),
            killswitch::controller(),
        )
    }

    pub fn with_provider_registry(config: Config, provider_registry: Arc<dyn ProviderRegistry>) -> Self {
        Self::with_dependencies(config, provider_registry, killswitch::controller())
    }

    pub fn with_dependencies(
        config: Config,
        provider_registry: Arc<dyn ProviderRegistry>,
        kill_switch_controller: Arc<dyn KillSwitchController>,
    ) -> Self {
        let kill_switch_active = config.general.kill_switch;
        Self {
            config,
            kill_switch_active,
            provider_registry,
            kill_switch_controller,
            config_path: None,
        }
    }

    pub fn with_config_path(mut self, config_path: PathBuf) -> Self {
        self.config_path = Some(config_path);
        self
    }

    pub fn provider(&self, kind: ProviderKind) -> Arc<dyn VpnProvider> {
        self.provider_registry.provider(kind)
    }

    pub fn providers(&self) -> Vec<Arc<dyn VpnProvider>> {
        self.provider_registry.all_providers()
    }

    pub fn kill_switch(&self) -> Arc<dyn KillSwitchController> {
        self.kill_switch_controller.clone()
    }

    pub fn save_config(&self) -> anyhow::Result<()> {
        match &self.config_path {
            Some(path) => self.config.save_to(path),
            None => self.config.save(),
        }
    }
}

pub struct StaticProviderRegistry {
    providers: HashMap<ProviderKind, Arc<dyn VpnProvider>>,
}

impl StaticProviderRegistry {
    pub fn new(providers: HashMap<ProviderKind, Arc<dyn VpnProvider>>) -> Self {
        Self { providers }
    }
}

impl ProviderRegistry for StaticProviderRegistry {
    fn provider(&self, kind: ProviderKind) -> Arc<dyn VpnProvider> {
        self.providers
            .get(&kind)
            .unwrap_or_else(|| panic!("missing provider registry entry for {kind}"))
            .clone()
    }

    fn all_providers(&self) -> Vec<Arc<dyn VpnProvider>> {
        ProviderKind::all()
            .iter()
            .filter_map(|kind| self.providers.get(kind).cloned())
            .collect()
    }
}
