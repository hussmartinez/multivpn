use crate::client;
use anyhow::{Result, bail};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use mvpn_core::ipc::{Request, Response};
use mvpn_core::types::{
    CreateRequest, FieldType, FormField, ProviderInfo, ProviderKind, SystemInfo, VpnConnection,
};
use ratatui::DefaultTerminal;
use std::time::Duration;

#[derive(Clone, Debug)]
pub enum Mode {
    Normal,
    Filter(String),
    Create(CreateForm),
    Import(ImportForm),
    ConfirmRemove(String),
    Providers,
    Config(String),
    SystemInfoView,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CreateStage {
    SelectProvider,
    EditFields,
}

#[derive(Clone, Debug)]
pub struct CreateFieldState {
    pub field: FormField,
    pub value: String,
}

#[derive(Clone, Debug)]
pub struct CreateForm {
    pub providers: Vec<ProviderInfo>,
    pub provider_index: usize,
    pub provider: Option<ProviderKind>,
    pub stage: CreateStage,
    pub selected: usize,
    pub name: String,
    pub fields: Vec<CreateFieldState>,
}

impl CreateForm {
    pub fn new(providers: Vec<ProviderInfo>) -> Self {
        Self {
            providers,
            provider_index: 0,
            provider: None,
            stage: CreateStage::SelectProvider,
            selected: 0,
            name: String::new(),
            fields: Vec::new(),
        }
    }

    pub fn selected_provider_info(&self) -> Option<&ProviderInfo> {
        self.providers.get(self.provider_index)
    }

    pub fn selected_provider(&self) -> Option<ProviderKind> {
        self.selected_provider_info().map(|provider| provider.kind)
    }

    pub fn next_provider(&mut self) {
        if !self.providers.is_empty() {
            self.provider_index = (self.provider_index + 1) % self.providers.len();
        }
    }

    pub fn previous_provider(&mut self) {
        if !self.providers.is_empty() {
            self.provider_index = if self.provider_index == 0 {
                self.providers.len() - 1
            } else {
                self.provider_index - 1
            };
        }
    }

    pub fn apply_fields(&mut self, provider: ProviderKind, fields: Vec<FormField>) {
        self.provider = Some(provider);
        self.stage = CreateStage::EditFields;
        self.selected = 0;
        self.name.clear();
        self.fields = fields
            .into_iter()
            .map(|field| {
                let value = if matches!(field.field_type, FieldType::Bool) {
                    "no".to_string()
                } else {
                    String::new()
                };
                CreateFieldState { field, value }
            })
            .collect();
    }

    pub fn title(&self) -> String {
        match self.provider_info() {
            Some(provider) => format!("Create Connection ({})", provider.display_name),
            None => "Create Connection".to_string(),
        }
    }

    pub fn provider_info(&self) -> Option<&ProviderInfo> {
        let kind = self.provider?;
        self.providers.iter().find(|provider| provider.kind == kind)
    }

    pub fn total_rows(&self) -> usize {
        1 + self.fields.len()
    }

    pub fn selected_label(&self) -> &str {
        if self.selected == 0 {
            "Name"
        } else {
            self.fields
                .get(self.selected - 1)
                .map(|state| state.field.label.as_str())
                .unwrap_or("")
        }
    }

    pub fn selected_value(&self) -> &str {
        if self.selected == 0 {
            &self.name
        } else {
            self.fields
                .get(self.selected - 1)
                .map(|state| state.value.as_str())
                .unwrap_or("")
        }
    }

    pub fn set_selected_value(&mut self, value: String) {
        if self.selected == 0 {
            self.name = value;
        } else if let Some(state) = self.fields.get_mut(self.selected - 1) {
            state.value = value;
        }
    }

    pub fn selected_field_type(&self) -> Option<FieldType> {
        if self.selected == 0 {
            None
        } else {
            self.fields
                .get(self.selected - 1)
                .map(|state| state.field.field_type.clone())
        }
    }

    pub fn is_bool_selected(&self) -> bool {
        matches!(self.selected_field_type(), Some(FieldType::Bool))
    }

    pub fn next(&mut self) {
        let total = self.total_rows();
        if total > 0 {
            self.selected = (self.selected + 1) % total;
        }
    }

    pub fn previous(&mut self) {
        let total = self.total_rows();
        if total > 0 {
            self.selected = if self.selected == 0 {
                total - 1
            } else {
                self.selected - 1
            };
        }
    }

    pub fn all_rows(&self) -> Vec<(String, String, bool)> {
        let mut rows = vec![("Name".to_string(), self.name.clone(), true)];
        rows.extend(self.fields.iter().map(|state| {
            let value = if matches!(state.field.field_type, FieldType::Secret) && !state.value.is_empty()
            {
                "*".repeat(state.value.chars().count())
            } else {
                state.value.clone()
            };
            (state.field.label.clone(), value, state.field.required)
        }));
        rows
    }

    pub fn request(&self) -> Result<(ProviderKind, CreateRequest)> {
        let provider = self.provider.ok_or_else(|| anyhow::anyhow!("provider not selected"))?;
        let name = self.name.trim().to_string();
        if name.is_empty() {
            bail!("name is required");
        }

        let mut fields = serde_json::Map::new();
        for state in &self.fields {
            let trimmed = state.value.trim();
            match state.field.field_type {
                FieldType::Bool => {
                    fields.insert(
                        state.field.key.clone(),
                        serde_json::Value::Bool(parse_bool(trimmed)),
                    );
                }
                _ => {
                    if state.field.required && trimmed.is_empty() {
                        bail!("{} is required", state.field.label);
                    }
                    if !trimmed.is_empty() {
                        fields.insert(
                            state.field.key.clone(),
                            serde_json::Value::String(trimmed.to_string()),
                        );
                    }
                }
            }
        }

        Ok((provider, CreateRequest { name, fields }))
    }
}

#[derive(Clone, Debug)]
pub struct ImportForm {
    pub providers: Vec<ProviderInfo>,
    pub provider_index: usize,
    pub path: String,
}

impl ImportForm {
    pub fn new(providers: Vec<ProviderInfo>, provider: ProviderKind) -> Self {
        let provider_index = providers
            .iter()
            .position(|item| item.kind == provider)
            .unwrap_or(0);
        Self {
            providers,
            provider_index,
            path: String::new(),
        }
    }

    pub fn current_provider(&self) -> Option<&ProviderInfo> {
        self.providers.get(self.provider_index)
    }

    pub fn provider_kind(&self) -> Option<ProviderKind> {
        self.current_provider().map(|provider| provider.kind)
    }

    pub fn next_provider(&mut self) {
        if !self.providers.is_empty() {
            self.provider_index = (self.provider_index + 1) % self.providers.len();
        }
    }

    pub fn previous_provider(&mut self) {
        if !self.providers.is_empty() {
            self.provider_index = if self.provider_index == 0 {
                self.providers.len() - 1
            } else {
                self.provider_index - 1
            };
        }
    }
}

pub struct App {
    pub connections: Vec<VpnConnection>,
    pub providers: Vec<ProviderInfo>,
    pub selected: usize,
    pub kill_switch: bool,
    pub message: String,
    pub should_quit: bool,
    pub mode: Mode,
    pub filter: String,
    pub daemon_available: bool,
    pub system_info: SystemInfo,
}

impl App {
    pub fn new() -> Self {
        Self {
            connections: Vec::new(),
            providers: Vec::new(),
            selected: 0,
            kill_switch: false,
            message: String::new(),
            should_quit: false,
            mode: Mode::Normal,
            filter: String::new(),
            daemon_available: false,
            system_info: SystemInfo::default(),
        }
    }

    pub fn run(&mut self) -> Result<()> {
        self.refresh();

        let mut terminal = ratatui::init();
        let result = self.event_loop(&mut terminal);
        ratatui::restore();
        result
    }

    fn event_loop(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        loop {
            terminal.draw(|frame| crate::ui::render(frame, self))?;

            if event::poll(Duration::from_millis(200))? {
                if let Event::Key(key) = event::read()? {
                    self.handle_key(key);
                }
            }

            if self.should_quit {
                break;
            }
        }
        Ok(())
    }

    fn handle_key(&mut self, key: KeyEvent) {
        match std::mem::replace(&mut self.mode, Mode::Normal) {
            Mode::Normal => self.handle_normal_key(key),
            Mode::Filter(mut text) => {
                if self.handle_filter_key(key, &mut text) {
                    self.mode = Mode::Filter(text);
                }
            }
            Mode::Create(mut form) => {
                if self.handle_create_key(key, &mut form) {
                    self.mode = Mode::Create(form);
                }
            }
            Mode::Import(mut form) => {
                if self.handle_import_key(key, &mut form) {
                    self.mode = Mode::Import(form);
                }
            }
            Mode::ConfirmRemove(name) => {
                if self.handle_confirm_remove_key(key, name.clone()) {
                    self.mode = Mode::ConfirmRemove(name);
                }
            }
            Mode::Providers => {
                if !matches!(key.code, KeyCode::Esc) {
                    self.mode = Mode::Providers;
                }
            }
            Mode::Config(text) => {
                if !matches!(key.code, KeyCode::Esc) {
                    self.mode = Mode::Config(text);
                }
            }
            Mode::SystemInfoView => match key.code {
                KeyCode::Char('r') => {
                    self.refresh_system_info();
                    self.mode = Mode::SystemInfoView;
                }
                KeyCode::Esc => {}
                _ => {
                    self.mode = Mode::SystemInfoView;
                }
            },
        }
    }

    fn handle_normal_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Down | KeyCode::Char('j') => self.select_next(),
            KeyCode::Up | KeyCode::Char('k') => self.select_previous(),
            KeyCode::Char('r') => self.refresh(),
            KeyCode::Char('c') => self.connect_selected(),
            KeyCode::Char('x') => self.disconnect_selected(),
            KeyCode::Char('a') => self.toggle_autostart_selected(),
            KeyCode::Char('d') => self.begin_remove_selected(),
            KeyCode::Char('n') => self.begin_create(),
            KeyCode::Char('i') => self.begin_import(),
            KeyCode::Char('/') => self.mode = Mode::Filter(self.filter.clone()),
            KeyCode::Char('K') => self.toggle_kill_switch(),
            KeyCode::Char('p') => self.mode = Mode::Providers,
            KeyCode::Char('e') => {
                let path = dirs_next::config_dir()
                    .unwrap_or_default()
                    .join("multivpn/config.toml");
                let text = std::fs::read_to_string(&path)
                    .unwrap_or_else(|_| format!("# Cannot read {}", path.display()));
                self.mode = Mode::Config(text);
            }
            KeyCode::Char('s') => {
                self.refresh_system_info();
                self.mode = Mode::SystemInfoView;
            }
            _ => {}
        }
    }

    fn handle_filter_key(&mut self, key: KeyEvent, text: &mut String) -> bool {
        match key.code {
            KeyCode::Esc => {
                self.filter.clear();
                self.selected = 0;
                false
            }
            KeyCode::Enter => {
                self.filter = text.clone();
                self.selected = 0;
                self.clamp_selection();
                false
            }
            KeyCode::Backspace => {
                text.pop();
                self.filter = text.clone();
                self.selected = 0;
                self.clamp_selection();
                true
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                text.push(c);
                self.filter = text.clone();
                self.selected = 0;
                self.clamp_selection();
                true
            }
            _ => true,
        }
    }

    fn handle_create_key(&mut self, key: KeyEvent, form: &mut CreateForm) -> bool {
        match form.stage {
            CreateStage::SelectProvider => match key.code {
                KeyCode::Esc => false,
                KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab | KeyCode::Right => {
                    form.next_provider();
                    true
                }
                KeyCode::Up | KeyCode::Char('k') | KeyCode::BackTab | KeyCode::Left => {
                    form.previous_provider();
                    true
                }
                KeyCode::Enter => {
                    if let Some(provider) = form.selected_provider() {
                        self.load_create_fields(form, provider);
                    }
                    true
                }
                _ => true,
            },
            CreateStage::EditFields => match key.code {
                KeyCode::Esc => false,
                KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab => {
                    form.next();
                    true
                }
                KeyCode::Up | KeyCode::Char('k') | KeyCode::BackTab => {
                    form.previous();
                    true
                }
                KeyCode::Enter if key.modifiers.is_empty() => {
                    self.submit_create(form);
                    !matches!(self.mode, Mode::Normal)
                }
                KeyCode::Char(' ') | KeyCode::Left | KeyCode::Right if form.is_bool_selected() => {
                    let toggled = !parse_bool(form.selected_value());
                    form.set_selected_value(bool_label(toggled).to_string());
                    true
                }
                KeyCode::Backspace => {
                    if !form.is_bool_selected() {
                        let mut value = form.selected_value().to_string();
                        value.pop();
                        form.set_selected_value(value);
                    }
                    true
                }
                KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    if form.is_bool_selected() {
                        if matches!(c, 'y' | 'Y' | 't' | 'T' | '1') {
                            form.set_selected_value("yes".to_string());
                        } else if matches!(c, 'n' | 'N' | 'f' | 'F' | '0') {
                            form.set_selected_value("no".to_string());
                        }
                    } else {
                        let mut value = form.selected_value().to_string();
                        value.push(c);
                        form.set_selected_value(value);
                    }
                    true
                }
                _ => true,
            },
        }
    }

    fn handle_import_key(&mut self, key: KeyEvent, form: &mut ImportForm) -> bool {
        match key.code {
            KeyCode::Esc => false,
            KeyCode::Backspace => {
                form.path.pop();
                true
            }
            KeyCode::Left => {
                form.previous_provider();
                true
            }
            KeyCode::Right => {
                form.next_provider();
                true
            }
            KeyCode::Enter => {
                self.submit_import(form);
                !matches!(self.mode, Mode::Normal)
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                form.path.push(c);
                true
            }
            _ => true,
        }
    }

    fn handle_confirm_remove_key(&mut self, key: KeyEvent, name: String) -> bool {
        match key.code {
            KeyCode::Esc | KeyCode::Char('n') => false,
            KeyCode::Char('y') => {
                self.remove_connection(name);
                !matches!(self.mode, Mode::Normal)
            }
            _ => true,
        }
    }

    fn refresh(&mut self) {
        self.refresh_connections();
        self.refresh_kill_switch();
        self.refresh_providers();
        self.refresh_system_info();
        self.clamp_selection();
    }

    fn sanitize(s: &str) -> String {
        mvpn_core::security::sanitize_output(s)
    }

    fn refresh_connections(&mut self) {
        match client::send(&Request::ListConnections) {
            Ok(Response::Connections { items }) => {
                self.daemon_available = true;
                self.connections = items;
                self.message = format!("{} connections", self.connections.len());
            }
            Ok(Response::Error { message }) => self.message = Self::sanitize(&message),
            Err(error) => self.set_daemon_error(&error),
            _ => {}
        }
    }

    fn refresh_kill_switch(&mut self) {
        match client::send(&Request::KillSwitchStatus) {
            Ok(Response::KillSwitch { active }) => self.kill_switch = active,
            Ok(Response::Error { message }) => self.message = Self::sanitize(&message),
            Err(error) => self.set_daemon_error(&error),
            _ => {}
        }
    }

    fn refresh_providers(&mut self) {
        match client::send(&Request::ListProviders) {
            Ok(Response::Providers { items }) => self.providers = items,
            Ok(Response::Error { message }) => self.message = Self::sanitize(&message),
            Err(error) => self.set_daemon_error(&error),
            _ => {}
        }
    }

    fn refresh_system_info(&mut self) {
        match client::send(&Request::SystemInfo) {
            Ok(Response::SystemInfo { info }) => self.system_info = info,
            _ => {}
        }
    }

    fn set_daemon_error(&mut self, error: &anyhow::Error) {
        let msg = error.to_string();
        self.daemon_available = false;
        if msg.contains("cannot connect") {
            self.message = "Daemon not running — start with: sudo systemctl start multivpn".to_string();
        } else if msg.to_lowercase().contains("permission denied") {
            self.message = "Permission denied — try running with sudo".to_string();
        } else {
            self.message = msg;
        }
    }

    fn clamp_selection(&mut self) {
        let filtered_len = self.filtered_connection_indices().len();
        if filtered_len == 0 {
            self.selected = 0;
        } else if self.selected >= filtered_len {
            self.selected = filtered_len - 1;
        }
    }

    fn select_next(&mut self) {
        let count = self.filtered_connection_indices().len();
        if count > 0 {
            self.selected = (self.selected + 1) % count;
        }
    }

    fn select_previous(&mut self) {
        let count = self.filtered_connection_indices().len();
        if count > 0 {
            self.selected = if self.selected == 0 {
                count - 1
            } else {
                self.selected - 1
            };
        }
    }

    fn connect_selected(&mut self) {
        if let Some(conn) = self.selected_connection().cloned() {
            let req = Request::Connect {
                provider: conn.provider,
                id: conn.id,
            };
            self.send_action(&req, true);
        }
    }

    fn disconnect_selected(&mut self) {
        if let Some(conn) = self.selected_connection().cloned() {
            let req = Request::Disconnect {
                provider: conn.provider,
                id: conn.id,
            };
            self.send_action(&req, true);
        }
    }

    fn toggle_autostart_selected(&mut self) {
        if let Some(conn) = self.selected_connection().cloned() {
            let req = Request::SetAutostart {
                provider: conn.provider,
                id: conn.id,
                enabled: !conn.autostart,
            };
            self.send_action(&req, true);
        }
    }

    fn begin_remove_selected(&mut self) {
        if let Some(conn) = self.selected_connection() {
            self.mode = Mode::ConfirmRemove(conn.name.clone());
        }
    }

    fn begin_create(&mut self) {
        let providers = self.available_providers();
        if providers.is_empty() {
            self.message = "no available providers".to_string();
            return;
        }
        self.mode = Mode::Create(CreateForm::new(providers));
    }

    fn begin_import(&mut self) {
        let providers = self.available_providers();
        if providers.is_empty() {
            self.message = "no available providers".to_string();
            return;
        }

        let preferred = self
            .selected_connection()
            .map(|conn| conn.provider)
            .unwrap_or_else(|| providers[0].kind);
        self.mode = Mode::Import(ImportForm::new(providers, preferred));
    }

    fn load_create_fields(&mut self, form: &mut CreateForm, provider: ProviderKind) {
        match client::send(&Request::GetConfigFields { provider }) {
            Ok(Response::ConfigFields { provider, fields }) => {
                form.apply_fields(provider, fields);
                self.message = format!("creating {}", provider);
            }
            Ok(Response::Error { message }) => self.message = message,
            Err(error) => self.message = error.to_string(),
            _ => self.message = "unexpected response while loading config fields".to_string(),
        }
    }

    fn submit_create(&mut self, form: &CreateForm) {
        match form.request() {
            Ok((provider, config)) => {
                let req = Request::Create { provider, config };
                self.mode = Mode::Normal;
                self.send_action(&req, true);
            }
            Err(error) => self.message = error.to_string(),
        }
    }

    fn submit_import(&mut self, form: &ImportForm) {
        let Some(provider) = form.provider_kind() else {
            self.message = "no provider selected".to_string();
            return;
        };
        let path = form.path.trim();
        if path.is_empty() {
            self.message = "path is required".to_string();
            return;
        }

        let req = Request::Import {
            provider,
            path: path.to_string(),
        };
        self.mode = Mode::Normal;
        self.send_action(&req, true);
    }

    fn remove_connection(&mut self, name: String) {
        if let Some(conn) = self.selected_connection().cloned() {
            let req = Request::Remove {
                provider: conn.provider,
                id: conn.id,
            };
            self.send_action(&req, true);
        } else {
            self.message = format!("cannot remove {name}: no connection selected");
        }
    }

    fn toggle_kill_switch(&mut self) {
        let req = if self.kill_switch {
            Request::KillSwitchDisable
        } else {
            Request::KillSwitchEnable
        };
        match client::send(&req) {
            Ok(Response::KillSwitch { active }) => {
                self.kill_switch = active;
                self.message = format!(
                    "Kill switch {}",
                    if active { "ENABLED" } else { "disabled" }
                );
            }
            Ok(Response::Error { message }) => self.message = message,
            Err(error) => self.message = error.to_string(),
            _ => {}
        }
    }

    fn send_action(&mut self, request: &Request, refresh_after: bool) {
        match client::send(request) {
            Ok(Response::Ok { message }) => {
                self.daemon_available = true;
                self.message = Self::sanitize(&message);
                if refresh_after {
                    self.refresh();
                }
            }
            Ok(Response::Error { message }) => self.message = Self::sanitize(&message),
            Err(error) => self.set_daemon_error(&error),
            _ => self.message = "unexpected response from daemon".to_string(),
        }
    }

    fn available_providers(&self) -> Vec<ProviderInfo> {
        self.providers
            .iter()
            .filter(|provider| provider.available)
            .cloned()
            .collect()
    }

    pub fn filtered_connection_indices(&self) -> Vec<usize> {
        if self.filter.is_empty() {
            return (0..self.connections.len()).collect();
        }

        let needle = self.filter.to_lowercase();
        self.connections
            .iter()
            .enumerate()
            .filter(|(_, conn)| conn.name.to_lowercase().contains(&needle))
            .map(|(index, _)| index)
            .collect()
    }

    pub fn filtered_connections(&self) -> Vec<&VpnConnection> {
        self.filtered_connection_indices()
            .into_iter()
            .filter_map(|index| self.connections.get(index))
            .collect()
    }

    pub fn selected_connection(&self) -> Option<&VpnConnection> {
        let index = *self.filtered_connection_indices().get(self.selected)?;
        self.connections.get(index)
    }

    pub fn filter_label(&self) -> Option<&str> {
        match &self.mode {
            Mode::Filter(text) => {
                if text.is_empty() {
                    None
                } else {
                    Some(text.as_str())
                }
            }
            _ => {
                if self.filter.is_empty() {
                    None
                } else {
                    Some(self.filter.as_str())
                }
            }
        }
    }
}

fn parse_bool(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "y" | "on"
    )
}

fn bool_label(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mvpn_core::types::ConnectionStatus;

    #[test]
    fn create_form_builds_request() {
        let providers = vec![ProviderInfo {
            kind: ProviderKind::WireGuard,
            display_name: "WireGuard".into(),
            available: true,
            install_hint: String::new(),
        }];
        let mut form = CreateForm::new(providers);
        form.apply_fields(
            ProviderKind::WireGuard,
            vec![
                FormField {
                    key: "addresses".into(),
                    label: "Addresses".into(),
                    required: false,
                    field_type: FieldType::Csv,
                },
                FormField {
                    key: "autostart".into(),
                    label: "Autostart".into(),
                    required: false,
                    field_type: FieldType::Bool,
                },
            ],
        );
        form.name = "wg0".into();
        form.fields[0].value = "10.0.0.2/24".into();
        form.fields[1].value = "yes".into();

        let (provider, request) = form.request().unwrap();
        assert_eq!(provider, ProviderKind::WireGuard);
        assert_eq!(request.name, "wg0");
        assert_eq!(request.fields["addresses"], "10.0.0.2/24");
        assert_eq!(request.fields["autostart"], true);
    }

    #[test]
    fn filtered_connections_are_case_insensitive() {
        let app = App {
            connections: vec![
                VpnConnection {
                    id: "wg0".into(),
                    provider: ProviderKind::WireGuard,
                    name: "Office".into(),
                    status: ConnectionStatus::Connected,
                    autostart: false,
                    details: serde_json::json!({}),
                    network: mvpn_core::types::NetworkInfo::default(),
                },
                VpnConnection {
                    id: "wg1".into(),
                    provider: ProviderKind::WireGuard,
                    name: "Home".into(),
                    status: ConnectionStatus::Disconnected,
                    autostart: false,
                    details: serde_json::json!({}),
                    network: mvpn_core::types::NetworkInfo::default(),
                },
            ],
            providers: Vec::new(),
            selected: 0,
            kill_switch: false,
            message: String::new(),
            should_quit: false,
            mode: Mode::Normal,
            filter: "off".into(),
            daemon_available: false,
            system_info: SystemInfo::default(),
        };

        let filtered = app.filtered_connections();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].name, "Office");
    }
}
