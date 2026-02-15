// QuectoClaw — TUI rendering (ratatui widgets and layout).

use super::app::{DashboardStats, LogEntry, LogLevel, SessionInfo};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Row, Table},
    Frame,
};

const HEADER_ART: &str = " QuectoClaw Dashboard ";
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Render the full dashboard layout.
pub fn render(
    frame: &mut Frame,
    logs: &[LogEntry],
    sessions: &[SessionInfo],
    stats: &DashboardStats,
) {
    let area = frame.area();

    // Main vertical split: header (3) | body | footer (3)
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(3),
        ])
        .split(area);

    render_header(frame, outer[0]);
    render_body(frame, outer[1], logs, sessions, stats);
    render_footer(frame, outer[2], stats);
}

fn render_header(frame: &mut Frame, area: Rect) {
    let title = Paragraph::new(Line::from(vec![
        Span::styled(
            "🦀 ",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            HEADER_ART,
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("v{}", VERSION),
            Style::default().fg(Color::DarkGray),
        ),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan)),
    );
    frame.render_widget(title, area);
}

fn render_body(
    frame: &mut Frame,
    area: Rect,
    logs: &[LogEntry],
    sessions: &[SessionInfo],
    stats: &DashboardStats,
) {
    // Horizontal split: stats+sessions (30%) | logs (70%)
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(area);

    // Left column: stats on top, sessions below
    let left = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(9), Constraint::Min(4)])
        .split(columns[0]);

    render_stats(frame, left[0], stats);
    render_sessions(frame, left[1], sessions);

    // Right column: logs
    render_logs(frame, columns[1], logs);
}

fn render_stats(frame: &mut Frame, area: Rect, stats: &DashboardStats) {
    let uptime = format_uptime(stats.uptime_secs);
    let lines = vec![
        Line::from(vec![
            Span::styled("  Requests  ", Style::default().fg(Color::Gray)),
            Span::styled(
                format!("{}", stats.total_requests),
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Tokens    ", Style::default().fg(Color::Gray)),
            Span::styled(
                format!("{}", stats.total_tokens),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Tools     ", Style::default().fg(Color::Gray)),
            Span::styled(
                format!("{}", stats.total_tool_calls),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Errors    ", Style::default().fg(Color::Gray)),
            Span::styled(
                format!("{}", stats.tool_errors),
                Style::default()
                    .fg(if stats.tool_errors > 0 {
                        Color::Red
                    } else {
                        Color::Green
                    })
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Channels  ", Style::default().fg(Color::Gray)),
            Span::styled(
                format!("{}", stats.active_channels),
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Uptime    ", Style::default().fg(Color::Gray)),
            Span::styled(
                uptime,
                Style::default()
                    .fg(Color::Blue)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
    ];

    let widget = Paragraph::new(lines).block(
        Block::default()
            .title(" ◉ Stats ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Green)),
    );
    frame.render_widget(widget, area);
}

fn render_sessions(frame: &mut Frame, area: Rect, sessions: &[SessionInfo]) {
    if sessions.is_empty() {
        let widget = Paragraph::new("  No active sessions")
            .style(Style::default().fg(Color::DarkGray))
            .block(
                Block::default()
                    .title(" ◉ Sessions ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Yellow)),
            );
        frame.render_widget(widget, area);
        return;
    }

    let rows: Vec<Row> = sessions
        .iter()
        .map(|s| {
            Row::new(vec![
                s.key.chars().take(12).collect::<String>(),
                s.channel.clone(),
                format!("{}", s.messages),
                s.last_activity.clone(),
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Min(12),
            Constraint::Length(8),
            Constraint::Length(5),
            Constraint::Length(8),
        ],
    )
    .header(
        Row::new(vec!["Session", "Channel", "Msgs", "Last"])
            .style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )
            .bottom_margin(1),
    )
    .block(
        Block::default()
            .title(" ◉ Sessions ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow)),
    );

    frame.render_widget(table, area);
}

fn render_logs(frame: &mut Frame, area: Rect, logs: &[LogEntry]) {
    let items: Vec<ListItem> = logs
        .iter()
        .rev()
        .take((area.height as usize).saturating_sub(2))
        .map(|entry| {
            let color = match entry.level {
                LogLevel::Info => Color::White,
                LogLevel::Warn => Color::Yellow,
                LogLevel::Error => Color::Red,
                LogLevel::Tool => Color::Cyan,
                LogLevel::Llm => Color::Magenta,
            };

            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("{} ", entry.timestamp),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(
                    format!("{} ", entry.level.symbol()),
                    Style::default().fg(color),
                ),
                Span::styled(entry.message.clone(), Style::default().fg(color)),
            ]))
        })
        .collect();

    let widget = List::new(items).block(
        Block::default()
            .title(" ◉ Activity Log ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Magenta)),
    );

    frame.render_widget(widget, area);
}

fn render_footer(frame: &mut Frame, area: Rect, _stats: &DashboardStats) {
    let footer = Paragraph::new(Line::from(vec![
        Span::styled(" q ", Style::default().fg(Color::Black).bg(Color::Cyan)),
        Span::styled(" Quit  ", Style::default().fg(Color::Gray)),
        Span::styled(" c ", Style::default().fg(Color::Black).bg(Color::Cyan)),
        Span::styled(" Clear logs  ", Style::default().fg(Color::Gray)),
        Span::styled(" Tab ", Style::default().fg(Color::Black).bg(Color::Cyan)),
        Span::styled(" Focus  ", Style::default().fg(Color::Gray)),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray)),
    );
    frame.render_widget(footer, area);
}

fn format_uptime(secs: u64) -> String {
    let hours = secs / 3600;
    let mins = (secs % 3600) / 60;
    let secs = secs % 60;
    format!("{:02}:{:02}:{:02}", hours, mins, secs)
}
