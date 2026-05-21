use crate::client;
use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent};
use mvpn_core::ipc::{Request, Response};
use mvpn_core::types::{ConnectionStatus, ProviderKind, VpnConnection};
use ratatui::DefaultTerminal;
use std::time::Duration;

pub struct App {
    pub connections: Vec<VpnConnection>,
    pub selected: usize,
    pub kill_switch: bool,
    pub message: String,
    pub should_quit: bool,
}

impl App {
    pub fn new() -> Self {
        Self {
            connections: Vec::new(),
            selected: 0,
            kill_switch: false,
            message: String::new(),
            should_quit: false,
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
        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Down | KeyCode::Char('j') => self.select_next(),
            KeyCode::Up | KeyCode::Char('k') => self.select_previous(),
            KeyCode::Char('r') => self.refresh(),
            KeyCode::Char('c') => self.connect_selected(),
            KeyCode::Char('x') => self.disconnect_selected(),
            KeyCode::Char('K') => self.toggle_kill_switch(),
            _ => {}
        }
    }

    fn select_next(&mut self) {
        if !self.connections.is_empty() {
            self.selected = (self.selected + 1) % self.connections.len();
        }
    }

    fn select_previous(&mut self) {
        if !self.connections.is_empty() {
            if self.selected == 0 {
                self.selected = self.connections.len() - 1;
            } else {
                self.selected -= 1;
            }
        }
    }

    fn refresh(&mut self) {
        match client::send(&Request::ListConnections) {
            Ok(Response::Connections { items }) => {
                self.connections = items;
                if self.selected >= self.connections.len() && !self.connections.is_empty() {
                    self.selected = self.connections.len() - 1;
                }
                self.message = format!("{} connections", self.connections.len());
            }
            Ok(Response::Error { message }) => self.message = message,
            Err(e) => self.message = e.to_string(),
            _ => {}
        }

        match client::send(&Request::KillSwitchStatus) {
            Ok(Response::KillSwitch { active }) => self.kill_switch = active,
            _ => {}
        }
    }

    fn connect_selected(&mut self) {
        if let Some(conn) = self.connections.get(self.selected) {
            let req = Request::Connect {
                provider: conn.provider,
                id: conn.id.clone(),
            };
            match client::send(&req) {
                Ok(Response::Ok { message }) => {
                    self.message = message;
                    self.refresh();
                }
                Ok(Response::Error { message }) => self.message = message,
                Err(e) => self.message = e.to_string(),
                _ => {}
            }
        }
    }

    fn disconnect_selected(&mut self) {
        if let Some(conn) = self.connections.get(self.selected) {
            let req = Request::Disconnect {
                provider: conn.provider,
                id: conn.id.clone(),
            };
            match client::send(&req) {
                Ok(Response::Ok { message }) => {
                    self.message = message;
                    self.refresh();
                }
                Ok(Response::Error { message }) => self.message = message,
                Err(e) => self.message = e.to_string(),
                _ => {}
            }
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
            Err(e) => self.message = e.to_string(),
            _ => {}
        }
    }

    pub fn selected_connection(&self) -> Option<&VpnConnection> {
        self.connections.get(self.selected)
    }
}
