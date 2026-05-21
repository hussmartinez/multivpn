use crate::types::{ConnectionStatus, CreateRequest, FormField, ProviderKind, VpnConnection};
use anyhow::Result;

pub trait VpnProvider: Send + Sync {
    fn kind(&self) -> ProviderKind;
    fn display_name(&self) -> &str;
    fn is_available(&self) -> bool;
    fn install_hint(&self) -> &str;
    fn list_connections(&self) -> Result<Vec<VpnConnection>>;
    fn connect(&self, id: &str) -> Result<()>;
    fn disconnect(&self, id: &str) -> Result<()>;
    fn status(&self, id: &str) -> Result<ConnectionStatus>;
    fn status_details(&self, id: &str) -> Result<String>;
    fn create(&self, config: &CreateRequest) -> Result<()>;
    fn remove(&self, id: &str) -> Result<()>;
    fn import(&self, path: &str) -> Result<String>;
    fn set_autostart(&self, id: &str, enabled: bool) -> Result<()>;
    fn config_fields(&self) -> Vec<FormField>;
}
