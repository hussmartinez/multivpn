use crate::app::App;
use mvpn_core::types::ConnectionStatus;
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};

pub fn render(frame: &mut Frame, app: &App) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(8),
            Constraint::Length(3),
        ])
        .split(frame.area());

    render_header(frame, layout[0], app);
    render_body(frame, layout[1], app);
    render_footer(frame, layout[2], app);
}

fn render_header(frame: &mut Frame, area: ratatui::layout::Rect, app: &App) {
    let ks_status = if app.kill_switch {
        "ACTIVE".to_string()
    } else {
        "off".to_string()
    };

    let title = Paragraph::new(Line::from(format!(
        "MultiVPN  |  Kill Switch: {ks_status}  |  Connections: {}",
        app.connections.len()
    )))
    .block(Block::default().borders(Borders::ALL).title("MultiVPN"));
    frame.render_widget(title, area);
}

fn render_body(frame: &mut Frame, area: ratatui::layout::Rect, app: &App) {
    let items: Vec<ListItem> = if app.connections.is_empty() {
        vec![ListItem::new(
            "No connections found. Is mvpn-daemon running?",
        )]
    } else {
        app.connections
            .iter()
            .map(|conn| {
                let status_icon = match &conn.status {
                    ConnectionStatus::Connected => "●",
                    ConnectionStatus::Disconnected => "○",
                    ConnectionStatus::Connecting => "◐",
                    ConnectionStatus::Error(_) => "✗",
                };
                let auto = if conn.autostart { " [auto]" } else { "" };
                ListItem::new(Line::from(format!(
                    "{status_icon} [{:<10}] {}{auto}",
                    conn.provider.as_str(),
                    conn.name,
                )))
            })
            .collect()
    };

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Connections"))
        .highlight_style(
            Style::default()
                .bg(Color::Blue)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("> ");

    let mut state = ListState::default();
    if !app.connections.is_empty() {
        state.select(Some(app.selected));
    }
    frame.render_stateful_widget(list, area, &mut state);
}

fn render_footer(frame: &mut Frame, area: ratatui::layout::Rect, app: &App) {
    let help = "[j/k] move  [c] connect  [x] disconnect  [K] kill switch  [r] refresh  [q] quit";
    let lines = vec![
        Line::from(app.message.clone()),
        Line::from(help.dark_gray()),
    ];
    let footer = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL))
        .wrap(Wrap { trim: true });
    frame.render_widget(footer, area);
}
