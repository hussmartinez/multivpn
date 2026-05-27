use crate::app::{App, CreateForm, CreateStage, ImportForm, Mode};
use mvpn_core::types::{ConnectionStatus, FieldType, VpnConnection};
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{
    Block, Borders, Clear, List, ListItem, ListState, Paragraph, Row, Table, Wrap,
};

const BORDER: Color = Color::DarkGray;
const TITLE: Color = Color::Cyan;
const GREEN: Color = Color::Green;
const RED: Color = Color::Red;
const YELLOW: Color = Color::Yellow;
const GRAY: Color = Color::DarkGray;
const HIGHLIGHT_BG: Color = Color::Blue;
const HIGHLIGHT_FG: Color = Color::White;

fn styled_block(title: &str) -> Block<'_> {
    Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(BORDER))
        .title(Span::styled(title, Style::default().fg(TITLE).add_modifier(Modifier::BOLD)))
}

fn status_color(status: &ConnectionStatus) -> Color {
    match status {
        ConnectionStatus::Connected => GREEN,
        ConnectionStatus::Disconnected => GRAY,
        ConnectionStatus::Connecting => YELLOW,
        ConnectionStatus::Error(_) => RED,
    }
}

fn status_icon(status: &ConnectionStatus) -> &'static str {
    match status {
        ConnectionStatus::Connected => "●",
        ConnectionStatus::Disconnected => "○",
        ConnectionStatus::Connecting => "◐",
        ConnectionStatus::Error(_) => "✗",
    }
}

pub fn render(frame: &mut Frame, app: &App) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(4),
        ])
        .split(frame.area());

    render_system_bar(frame, layout[0], app);
    render_body(frame, layout[1], app);
    render_footer(frame, layout[2], app);

    match &app.mode {
        Mode::Create(form) => render_create_modal(frame, centered_rect(72, 72, frame.area()), form),
        Mode::Import(form) => render_import_modal(frame, centered_rect(60, 24, frame.area()), form),
        Mode::ConfirmRemove(name) => {
            render_confirm_modal(frame, centered_rect(56, 24, frame.area()), name)
        }
        Mode::Providers => render_providers_modal(frame, centered_rect(70, 60, frame.area()), app),
        Mode::Config(text) => render_config_modal(frame, centered_rect(80, 80, frame.area()), text),
        Mode::SystemInfoView => render_system_info_modal(frame, centered_rect(70, 50, frame.area()), app),
        Mode::Normal | Mode::Filter(_) => {}
    }
}

fn render_system_bar(frame: &mut Frame, area: Rect, app: &App) {
    let pub_ip = app.system_info.public_ip.as_deref().unwrap_or("—");
    let gw = app.system_info.default_gateway.as_deref().unwrap_or("—");
    let gw_iface = app.system_info.default_interface.as_deref().unwrap_or("—");

    let connected_count = app.connections.iter()
        .filter(|c| matches!(c.status, ConnectionStatus::Connected))
        .count();

    let filter = app.filter_label()
        .map(|text| format!("  Filter: {text}"))
        .unwrap_or_default();

    let ks_span = if app.kill_switch {
        Span::styled(" KS: ON ", Style::default().fg(Color::White).bg(RED).add_modifier(Modifier::BOLD))
    } else {
        Span::styled("KS: off", Style::default().fg(GRAY))
    };

    let line = Line::from(vec![
        Span::styled("IP: ", Style::default().fg(GRAY)),
        Span::styled(pub_ip, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        Span::raw("  "),
        Span::styled("GW: ", Style::default().fg(GRAY)),
        Span::styled(format!("{gw} via {gw_iface}"), Style::default().fg(Color::White)),
        Span::raw("  "),
        ks_span,
        Span::raw("  "),
        Span::styled(format!("{connected_count}/{} connected", app.connections.len()), Style::default().fg(GREEN)),
        Span::styled(filter, Style::default().fg(YELLOW)),
    ]);

    let bar = Paragraph::new(line).block(styled_block("System"));
    frame.render_widget(bar, area);
}

fn render_body(frame: &mut Frame, area: Rect, app: &App) {
    let layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
        .split(area);

    render_list(frame, layout[0], app);
    render_details(frame, layout[1], app);
}

fn render_list(frame: &mut Frame, area: Rect, app: &App) {
    let filtered = app.filtered_connections();
    let items: Vec<ListItem> = if filtered.is_empty() {
        vec![ListItem::new(
            Line::from(Span::styled("No connections found", Style::default().fg(GRAY)))
        )]
    } else {
        filtered
            .iter()
            .map(|conn| {
                let color = status_color(&conn.status);
                let icon = status_icon(&conn.status);
                let auto = if conn.autostart { " ⟳" } else { "" };
                let iface = conn.network.interface.as_deref().unwrap_or("");
                let iface_str = if iface.is_empty() { String::new() } else { format!(" ({iface})") };
                ListItem::new(Line::from(vec![
                    Span::styled(icon, Style::default().fg(color)),
                    Span::raw(" "),
                    Span::styled(format!("[{:<10}]", conn.provider.as_str()), Style::default().fg(GRAY)),
                    Span::raw(" "),
                    Span::styled(&conn.name, Style::default().fg(Color::White)),
                    Span::styled(iface_str, Style::default().fg(GRAY)),
                    Span::styled(auto, Style::default().fg(YELLOW)),
                ]))
            })
            .collect()
    };

    let title = match app.filter_label() {
        Some(text) => format!("Connections [filter: {text}]"),
        None => "Connections".to_string(),
    };

    let list = List::new(items)
        .block(styled_block(&title))
        .highlight_style(
            Style::default()
                .bg(HIGHLIGHT_BG)
                .fg(HIGHLIGHT_FG)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▸ ");

    let mut state = ListState::default();
    if !filtered.is_empty() {
        state.select(Some(app.selected));
    }
    frame.render_stateful_widget(list, area, &mut state);
}

fn render_details(frame: &mut Frame, area: Rect, app: &App) {
    if let Some(conn) = app.selected_connection() {
        let vertical = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(12), Constraint::Min(6)])
            .split(area);

        render_connection_info(frame, vertical[0], conn);
        render_metadata(frame, vertical[1], conn);
    } else {
        let text = Paragraph::new(Span::styled("No connection selected", Style::default().fg(GRAY)))
            .block(styled_block("Details"))
            .alignment(Alignment::Center);
        frame.render_widget(text, area);
    }
}

fn render_connection_info(frame: &mut Frame, area: Rect, conn: &VpnConnection) {
    let color = status_color(&conn.status);
    let net = &conn.network;

    let mut rows = vec![
        detail_row("Provider", &conn.provider.to_string(), TITLE),
        detail_row("Status", &connection_status_text(conn), color),
        detail_row("Name", &conn.name, Color::White),
        detail_row("Autostart", yes_no(conn.autostart), if conn.autostart { GREEN } else { GRAY }),
    ];

    if let Some(ref iface) = net.interface {
        rows.push(detail_row("Interface", iface, Color::White));
    }
    if let Some(ref ip) = net.local_ip {
        rows.push(detail_row("Local IP", ip, Color::White));
    }
    if let Some(ref gw) = net.gateway {
        rows.push(detail_row("Gateway", gw, Color::White));
    }
    if let Some(ref ep) = net.endpoint {
        rows.push(detail_row("Endpoint", ep, Color::White));
    }
    if !net.dns.is_empty() {
        rows.push(detail_row("DNS", &net.dns.join(", "), Color::White));
    }
    if !net.routes.is_empty() {
        rows.push(detail_row("Routes", &net.routes.join(", "), Color::White));
    }
    if net.transfer_rx.is_some() || net.transfer_tx.is_some() {
        let rx = net.transfer_rx.as_deref().unwrap_or("—");
        let tx = net.transfer_tx.as_deref().unwrap_or("—");
        rows.push(detail_row("Transfer", &format!("↓ {rx}  ↑ {tx}"), YELLOW));
    }

    let table = Table::new(rows, [Constraint::Length(14), Constraint::Min(24)])
        .block(styled_block("Details"))
        .column_spacing(1);
    frame.render_widget(table, area);
}

fn detail_row(label: &str, value: &str, value_color: Color) -> Row<'static> {
    Row::new(vec![
        Span::styled(label.to_string(), Style::default().fg(GRAY)),
        Span::styled(value.to_string(), Style::default().fg(value_color)),
    ])
}

fn render_metadata(frame: &mut Frame, area: Rect, conn: &VpnConnection) {
    let text = format_details(&conn.details);
    let paragraph = Paragraph::new(text)
        .block(styled_block("Metadata"))
        .wrap(Wrap { trim: false })
        .style(Style::default().fg(GRAY));
    frame.render_widget(paragraph, area);
}

fn render_footer(frame: &mut Frame, area: Rect, app: &App) {
    let help_spans = match &app.mode {
        Mode::Normal => build_help_spans(&[
            ("j/k", "move"), ("c", "connect"), ("x", "disconnect"), ("a", "auto"),
            ("n", "new"), ("i", "import"), ("d", "delete"), ("/", "filter"),
            ("K", "kill switch"), ("p", "providers"), ("e", "config"),
            ("s", "system"), ("r", "refresh"), ("q", "quit"),
        ]),
        Mode::Filter(_) => build_help_spans(&[
            ("type", "filter"), ("enter", "apply"), ("esc", "clear"),
        ]),
        Mode::Create(form) => match form.stage {
            CreateStage::SelectProvider => build_help_spans(&[
                ("j/k", "move"), ("enter", "select"), ("esc", "cancel"),
            ]),
            CreateStage::EditFields => build_help_spans(&[
                ("tab/↑/↓", "field"), ("enter", "create"), ("esc", "cancel"), ("space", "toggle"),
            ]),
        },
        Mode::Import(_) => build_help_spans(&[
            ("←/→", "provider"), ("type", "path"), ("enter", "import"), ("esc", "cancel"),
        ]),
        Mode::ConfirmRemove(_) => build_help_spans(&[
            ("y", "confirm"), ("n/esc", "cancel"),
        ]),
        Mode::Providers => build_help_spans(&[("esc", "back")]),
        Mode::Config(_) => build_help_spans(&[("esc", "back")]),
        Mode::SystemInfoView => build_help_spans(&[("r", "refresh"), ("esc", "back")]),
    };

    let msg_style = if app.message.contains("error") || app.message.contains("Error") || app.message.contains("denied") {
        Style::default().fg(RED)
    } else if app.message.contains("connected") || app.message.contains("enabled") || app.message.contains("created") {
        Style::default().fg(GREEN)
    } else {
        Style::default().fg(Color::White)
    };

    let lines = vec![
        Line::from(Span::styled(app.message.clone(), msg_style)),
        Line::from(help_spans),
    ];
    let footer = Paragraph::new(lines)
        .block(styled_block("Actions"))
        .wrap(Wrap { trim: true });
    frame.render_widget(footer, area);
}

fn build_help_spans(items: &[(&str, &str)]) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    for (i, (key, desc)) in items.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled("  ", Style::default()));
        }
        spans.push(Span::styled(format!("[{key}]"), Style::default().fg(YELLOW)));
        spans.push(Span::styled(format!(" {desc}"), Style::default().fg(GRAY)));
    }
    spans
}

fn render_providers_modal(frame: &mut Frame, area: Rect, app: &App) {
    frame.render_widget(Clear, area);

    let rows: Vec<Row> = app.providers.iter().map(|p| {
        let hint = if p.available {
            String::new()
        } else {
            p.install_hint.clone()
        };
        Row::new(vec![
            p.display_name.clone(),
            if p.available { "✓ installed".into() } else { "✗ missing".into() },
            hint,
        ]).style(if p.available {
            Style::default().fg(GREEN)
        } else {
            Style::default().fg(GRAY)
        })
    }).collect();

    let table = Table::new(
        rows,
        [Constraint::Length(14), Constraint::Length(12), Constraint::Min(30)],
    )
    .header(
        Row::new(vec!["Provider", "Status", "Install Command"])
            .style(Style::default().fg(TITLE).add_modifier(Modifier::BOLD))
    )
    .block(styled_block("Providers"))
    .column_spacing(2);
    frame.render_widget(table, area);
}

fn render_config_modal(frame: &mut Frame, area: Rect, config_text: &str) {
    frame.render_widget(Clear, area);

    let path = dirs_next::config_dir()
        .unwrap_or_default()
        .join("multivpn/config.toml");
    let title = format!("Config — {}", path.display());

    let paragraph = Paragraph::new(config_text.to_string())
        .block(styled_block(&title))
        .wrap(Wrap { trim: false })
        .style(Style::default().fg(Color::White));
    frame.render_widget(paragraph, area);
}

fn render_system_info_modal(frame: &mut Frame, area: Rect, app: &App) {
    frame.render_widget(Clear, area);

    let si = &app.system_info;
    let pub_ip = si.public_ip.as_deref().unwrap_or("unknown");
    let gw = si.default_gateway.as_deref().unwrap_or("unknown");
    let iface = si.default_interface.as_deref().unwrap_or("unknown");

    let connected: Vec<&VpnConnection> = app.connections.iter()
        .filter(|c| matches!(c.status, ConnectionStatus::Connected))
        .collect();

    let mut lines = vec![
        Line::from(vec![
            Span::styled("Public IP:  ", Style::default().fg(GRAY)),
            Span::styled(pub_ip, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled("Gateway:    ", Style::default().fg(GRAY)),
            Span::styled(gw, Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled("Interface:  ", Style::default().fg(GRAY)),
            Span::styled(iface, Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled("Kill Switch:", Style::default().fg(GRAY)),
            Span::raw(" "),
            if app.kill_switch {
                Span::styled("ACTIVE", Style::default().fg(RED).add_modifier(Modifier::BOLD))
            } else {
                Span::styled("off", Style::default().fg(GRAY))
            },
        ]),
        Line::from(""),
        Line::from(Span::styled(
            format!("Active VPN Connections ({}):", connected.len()),
            Style::default().fg(TITLE).add_modifier(Modifier::BOLD),
        )),
    ];

    for conn in &connected {
        let net = &conn.network;
        let ip = net.local_ip.as_deref().unwrap_or("—");
        let via = net.interface.as_deref().unwrap_or("—");
        lines.push(Line::from(vec![
            Span::styled("  ● ", Style::default().fg(GREEN)),
            Span::styled(conn.name.clone(), Style::default().fg(Color::White)),
            Span::styled(format!("  {ip} via {via}"), Style::default().fg(GRAY)),
        ]));
    }

    if connected.is_empty() {
        lines.push(Line::from(Span::styled("  No active connections", Style::default().fg(GRAY))));
    }

    let paragraph = Paragraph::new(lines)
        .block(styled_block("System Info"))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

fn render_create_modal(frame: &mut Frame, area: Rect, form: &CreateForm) {
    frame.render_widget(Clear, area);

    match form.stage {
        CreateStage::SelectProvider => render_provider_modal(frame, area, form),
        CreateStage::EditFields => render_create_fields_modal(frame, area, form),
    }
}

fn render_provider_modal(frame: &mut Frame, area: Rect, form: &CreateForm) {
    let items: Vec<ListItem> = form
        .providers
        .iter()
        .map(|provider| ListItem::new(Line::from(provider.display_name.clone())))
        .collect();

    let list = List::new(items)
        .block(styled_block("Create Connection: Select Provider"))
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(YELLOW)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▸ ");

    let mut state = ListState::default();
    if !form.providers.is_empty() {
        state.select(Some(form.provider_index));
    }
    frame.render_stateful_widget(list, area, &mut state);
}

fn render_create_fields_modal(frame: &mut Frame, area: Rect, form: &CreateForm) {
    let title = form.title();
    let outer = styled_block(&title)
        .style(Style::default().bg(Color::Black));
    frame.render_widget(outer, area);

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([Constraint::Min(8), Constraint::Length(5)])
        .split(area);

    let rows: Vec<Row> = form
        .all_rows()
        .into_iter()
        .enumerate()
        .map(|(index, (label, value, required))| {
            let marker = if required { "*" } else { "" };
            let style = if index == form.selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(YELLOW)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            Row::new(vec![format!("{label}{marker}"), value]).style(style)
        })
        .collect();

    let table = Table::new(rows, [Constraint::Length(24), Constraint::Min(24)])
        .block(styled_block("Fields"))
        .column_spacing(1);
    frame.render_widget(table, inner[0]);

    let field_type = match form.selected_field_type() {
        Some(FieldType::Text) => "text",
        Some(FieldType::Secret) => "secret",
        Some(FieldType::Bool) => "bool",
        Some(FieldType::Csv) => "csv",
        None => "text",
    };
    let hints = Paragraph::new(Text::from(vec![
        Line::from(format!(
            "Provider: {}",
            form.provider_info()
                .map(|provider| provider.display_name.as_str())
                .unwrap_or("-")
        )),
        Line::from(format!("Current field: {}", form.selected_label())),
        Line::from(format!("Field type: {field_type}")),
        Line::from(Span::styled("Fields marked with * are required", Style::default().fg(GRAY))),
    ]))
    .block(styled_block("Hints"))
    .wrap(Wrap { trim: true });
    frame.render_widget(hints, inner[1]);
}

fn render_import_modal(frame: &mut Frame, area: Rect, form: &ImportForm) {
    frame.render_widget(Clear, area);
    let provider = form
        .current_provider()
        .map(|provider| provider.display_name.as_str())
        .unwrap_or("-");
    let text = Paragraph::new(Text::from(vec![
        Line::from(format!("Provider: {provider}")),
        Line::from("Enter path to import:"),
        Line::from(""),
        Line::from(Span::styled(format!("> {}_", form.path), Style::default().fg(Color::White))),
    ]))
    .block(styled_block("Import Config"))
    .wrap(Wrap { trim: true });
    frame.render_widget(text, area);
}

fn render_confirm_modal(frame: &mut Frame, area: Rect, name: &str) {
    frame.render_widget(Clear, area);
    let text = Paragraph::new(vec![
        Line::from(Span::styled(
            format!("Remove connection `{name}`?"),
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled("Press y to confirm.", Style::default().fg(YELLOW))),
    ])
    .block(styled_block("Confirm Removal"))
    .alignment(Alignment::Center)
    .wrap(Wrap { trim: true });
    frame.render_widget(text, area);
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

fn connection_status_text(conn: &VpnConnection) -> String {
    match &conn.status {
        ConnectionStatus::Connected => "connected".to_string(),
        ConnectionStatus::Disconnected => "disconnected".to_string(),
        ConnectionStatus::Connecting => "connecting…".to_string(),
        ConnectionStatus::Error(error) => format!("error: {error}"),
    }
}

fn format_details(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Object(map) if map.is_empty() => String::new(),
        _ => serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string()),
    }
}
