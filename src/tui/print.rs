use crate::process::{Process, ProcessStatus};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState},
};
use std::io::Stdout;

pub type Term = Terminal<CrosstermBackend<Stdout>>;

pub fn draw(
    terminal: &mut Term,
    processes: &[Process],
    table_state: &mut TableState,
) -> anyhow::Result<()> {
    terminal.draw(|frame| {
        let area = frame.area();

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(1)])
            .split(area);

        let header = Row::new(vec![
            "id", "name", "mode", "pid", "cpu%", "mem", "uptime", "status", "watch", "↺",
        ])
        .style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
        .height(1);

        let rows: Vec<Row> = processes
            .iter()
            .map(|p| {
                let status_style = match p.status {
                    ProcessStatus::Online => Style::default().fg(Color::Green),
                    ProcessStatus::Stopped => Style::default().fg(Color::Red),
                };
                let status_label = match p.status {
                    ProcessStatus::Online => "● online",
                    ProcessStatus::Stopped => "○ stopped",
                };

                Row::new(vec![
                    Cell::from(p.id.to_string()),
                    Cell::from(p.name.clone()),
                    Cell::from(p.mode.clone()),
                    Cell::from(p.pid.map(|p| p.to_string()).unwrap_or("-".into())),
                    Cell::from(format!("{:.1}", p.cpu)),
                    Cell::from(p.format_mem()),
                    Cell::from(p.format_uptime()),
                    Cell::from(status_label).style(status_style),
                    Cell::from(if p.watching { "yes" } else { "no" }),
                    Cell::from(p.restarts.to_string()),
                ])
            })
            .collect();

        let widths = [
            Constraint::Length(4),
            Constraint::Min(12),
            Constraint::Length(8),
            Constraint::Length(7),
            Constraint::Length(6),
            Constraint::Length(8),
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Length(6),
            Constraint::Length(4),
        ];

        let table = Table::new(rows, widths)
            .header(header)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" rpm2 ")
                    .title_style(
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    ),
            )
            .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED))
            .column_spacing(1);

        frame.render_stateful_widget(table, chunks[0], table_state);

        let footer =
            Paragraph::new(" ↑/↓: select  s: stop  r: restart  d: delete  w: watch  q: quit ")
                .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(footer, chunks[1]);
    })?;

    Ok(())
}
