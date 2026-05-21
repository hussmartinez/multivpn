use crate::app::{App, CreateForm, CreateStage, ImportForm, Mode};
use mvpn_core::types::{ConnectionStatus, FieldType};
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Text};
use ratatui::widgets::{
    Block, Borders, Clear, List, ListItem, ListState, Paragraph, Row, Table, Wrap,
};

pub fn render(frame: &mut Frame, app: &App) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(3),
        ])
        .split(frame.area());

    render_header(frame, layout[0], app);
    render_body(frame, layout[1], app);
    render_footer(frame, layout[2], app);

    match &app.mode {
        Mode::Create(form) => render_create_modal(frame, centered_rect(72, 72, frame.area()), form),
        Mode::Import(form) => render_import_modal(frame, centered_rect(60, 24, frame.area()), form),
        Mode::ConfirmRemove(name) => {
            render_confirm_modal(frame, centered_rect(56, 24, frame.area()), name)
        }
        Mode::Normal | Mode::Filter(_) => {}
    }
}

fn render_header(frame: &mut Frame, area: Rect, app: &App) {
    let ks_status = if app.kill_switch { "ACTIVE" } else { "off" };
    let filter = app
        .filter_label()
        .map(|text| format!("  |  Filter: {text}"))
        .unwrap_or_default();

    let title = Paragraph::new(Line::from(format!(
        "MultiVPN  |  Kill Switch: {ks_status}  |  Connections: {}{}",
        app.filtered_connections().len(),
        filter
    )))
    .block(Block::default().borders(Borders::ALL).title("Overview"));
    frame.render_widget(title, area);
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
        vec![ListItem::new("No connections found. Is mvpn-daemon running?")]
    } else {
        filtered
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

    let title = match app.filter_label() {
        Some(text) => format!("Connections [filter: {text}]"),
        None => "Connections".to_string(),
    };

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(
            Style::default()
                .bg(Color::Blue)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("> ");

    let mut state = ListState::default();
    if !filtered.is_empty() {
        state.select(Some(app.selected));
    }
    frame.render_stateful_widget(list, area, &mut state);
}

fn render_details(frame: &mut Frame, area: Rect, app: &App) {
    let block = Block::default().borders(Borders::ALL).title("Details");
    if let Some(conn) = app.selected_connection() {
        let vertical = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(7), Constraint::Min(8)])
            .split(area);

        let summary_rows = vec![
            Row::new(vec!["Provider".to_string(), conn.provider.to_string()]),
            Row::new(vec!["Name".to_string(), conn.name.clone()]),
            Row::new(vec!["Status".to_string(), connection_status(conn)]),
            Row::new(vec![
                "Autostart".to_string(),
                yes_no(conn.autostart).to_string(),
            ]),
        ];
        let table = Table::new(summary_rows, [Constraint::Length(12), Constraint::Min(24)])
            .block(block)
            .column_spacing(1);
        frame.render_widget(table, vertical[0]);

        let details = Paragraph::new(format_details(&conn.details))
            .block(Block::default().borders(Borders::ALL).title("Metadata"))
            .wrap(Wrap { trim: false });
        frame.render_widget(details, vertical[1]);
    } else {
        let text = Paragraph::new("No connection selected")
            .block(block)
            .alignment(Alignment::Center);
        frame.render_widget(text, area);
    }
}

fn render_footer(frame: &mut Frame, area: Rect, app: &App) {
    let help = match &app.mode {
        Mode::Normal => {
            "[j/k] move  [c] connect  [x] disconnect  [a] autostart  [n] new  [i] import  [d] delete  [/] filter  [K] kill switch  [r] refresh  [q] quit"
        }
        Mode::Filter(_) => "Type to filter  [enter] accept  [esc] clear",
        Mode::Create(form) => match form.stage {
            CreateStage::SelectProvider => "[j/k or tab] provider  [enter] select  [esc] cancel",
            CreateStage::EditFields => {
                "[tab/↑/↓] field  [enter] create  [esc] cancel  [space] toggle bool"
            }
        },
        Mode::Import(_) => "[←/→] provider  type path  [enter] import  [esc] cancel",
        Mode::ConfirmRemove(_) => "[y] confirm remove  [n/esc] cancel",
    };

    let lines = vec![
        Line::from(app.message.clone()),
        Line::from(help.dark_gray()),
    ];
    let footer = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title("Actions"))
        .wrap(Wrap { trim: true });
    frame.render_widget(footer, area);
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
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Create Connection: Select Provider"),
        )
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("> ");

    let mut state = ListState::default();
    if !form.providers.is_empty() {
        state.select(Some(form.provider_index));
    }
    frame.render_stateful_widget(list, area, &mut state);
}

fn render_create_fields_modal(frame: &mut Frame, area: Rect, form: &CreateForm) {
    let outer = Block::default()
        .borders(Borders::ALL)
        .title(form.title())
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
                    .bg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            Row::new(vec![format!("{label}{marker}"), value]).style(style)
        })
        .collect();

    let table = Table::new(rows, [Constraint::Length(24), Constraint::Min(24)])
        .block(Block::default().borders(Borders::ALL).title("Fields"))
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
        Line::from("Fields marked with * are required".dark_gray()),
    ]))
    .block(Block::default().borders(Borders::ALL).title("Hints"))
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
        Line::from(format!("> {}_", form.path)),
    ]))
    .block(Block::default().borders(Borders::ALL).title("Import Config"))
    .wrap(Wrap { trim: true });
    frame.render_widget(text, area);
}

fn render_confirm_modal(frame: &mut Frame, area: Rect, name: &str) {
    frame.render_widget(Clear, area);
    let text = Paragraph::new(format!(
        "Remove connection `{name}`?\n\nPress y to confirm."
    ))
    .block(Block::default().borders(Borders::ALL).title("Confirm Removal"))
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

fn connection_status(conn: &mvpn_core::types::VpnConnection) -> String {
    match &conn.status {
        ConnectionStatus::Connected => "connected".to_string(),
        ConnectionStatus::Disconnected => "disconnected".to_string(),
        ConnectionStatus::Connecting => "connecting".to_string(),
        ConnectionStatus::Error(error) => format!("error: {error}"),
    }
}

fn format_details(value: &serde_json::Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}
