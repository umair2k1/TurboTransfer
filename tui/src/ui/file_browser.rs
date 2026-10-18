use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use ratatui::Frame;

use crate::app::AppState;

pub fn render_file_browser(f: &mut Frame, app: &AppState, area: Rect) {
    let has_queue = !app.send_queue.is_empty();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Path banner
            Constraint::Min(8),    // File & Folder list
            if has_queue {
                Constraint::Length(3)
            } else {
                Constraint::Length(0)
            }, // Queue footer
        ])
        .split(area);

    // Current Directory Banner & Search Pill
    let mut banner_spans = vec![
        Span::styled(" Directory: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            app.browser_current_dir.display().to_string(),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
    ];

    if !app.file_search_query.is_empty() {
        banner_spans.push(Span::styled(
            "   🔍 Filter: ",
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ));
        banner_spans.push(Span::styled(
            format!(" \"{}\" ", app.file_search_query),
            Style::default()
                .fg(Color::Black)
                .bg(Color::White)
                .add_modifier(Modifier::BOLD),
        ));
        banner_spans.push(Span::styled(
            " (Type to filter, Backspace/Esc to clear)",
            Style::default().fg(Color::DarkGray),
        ));
    }

    let banner_text = Line::from(banner_spans);
    let banner_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::DarkGray));
    let banner_para = Paragraph::new(banner_text).block(banner_block);
    f.render_widget(banner_para, chunks[0]);

    // File list
    let list_title = if has_queue {
        format!(
            " Files & Folders [Queue: {} files ({})] (Space: Select | Tab/S: Send | Enter: Open) ",
            app.send_queue.len(),
            format_bytes(app.queue_total_bytes())
        )
    } else {
        " Files & Folders (Space: Select File/Folder | Enter: Open/Select | Backspace: Up) "
            .to_string()
    };
    let list_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(list_title);

    let mut lines = Vec::new();

    // Up directory entry
    let is_parent_selected = app.browser_selected_index == 0;
    let parent_prefix = if is_parent_selected { " ► " } else { "   " };
    lines.push(Line::from(vec![
        Span::styled(
            parent_prefix,
            if is_parent_selected {
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
            },
        ),
        Span::styled("📁 ", Style::default().fg(Color::White)),
        Span::styled(
            ".. (Parent Directory)",
            if is_parent_selected {
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            },
        ),
    ]));

    for (i, path) in app.browser_entries.iter().enumerate() {
        let is_selected = (i + 1) == app.browser_selected_index;
        let prefix = if is_selected { " ► " } else { "   " };

        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("?");
        let is_dir = path.is_dir();
        let is_in_queue = if is_dir {
            !app.send_queue.is_empty() && app.send_queue.iter().any(|f| f.starts_with(path))
        } else {
            app.send_queue.contains(path)
        };

        let mark_span = if is_in_queue {
            Span::styled(
                "[✓] ",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            Span::styled("[ ] ", Style::default().fg(Color::DarkGray))
        };

        let (icon, size_str, style) = if is_dir {
            (
                "📁 ",
                "<DIR>".to_string(),
                if is_selected {
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                },
            )
        } else {
            let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
            (
                "📄 ",
                format_bytes(size),
                if is_selected {
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::Gray)
                },
            )
        };

        lines.push(Line::from(vec![
            Span::styled(
                prefix,
                if is_selected {
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::DarkGray)
                },
            ),
            mark_span,
            Span::styled(icon, Style::default().fg(Color::White)),
            Span::styled(format!("{:<40}", name), style),
            Span::styled(
                format!("{:>14}", size_str),
                Style::default().fg(Color::DarkGray),
            ),
        ]));
    }

    if app.browser_entries.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "   (Empty directory)",
            Style::default().fg(Color::DarkGray),
        )));
    }

    let visible_rows = (chunks[1].height.saturating_sub(2) as usize).max(1);
    let scroll_y = if app.browser_selected_index >= visible_rows {
        (app.browser_selected_index - visible_rows + 1) as u16
    } else {
        0
    };

    let list_para = Paragraph::new(lines)
        .block(list_block)
        .scroll((scroll_y, 0));
    f.render_widget(list_para, chunks[1]);

    if has_queue {
        let queue_spans = vec![
            Span::styled(
                " 📦 Send Queue: ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(
                    "{} file(s) ({})",
                    app.send_queue.len(),
                    format_bytes(app.queue_total_bytes())
                ),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" │ Press ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                "[Tab] / [S]",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" to Send, ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                "[C]",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" to Clear", Style::default().fg(Color::DarkGray)),
        ];
        let queue_block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Cyan));
        let queue_para = Paragraph::new(Line::from(queue_spans)).block(queue_block);
        f.render_widget(queue_para, chunks[2]);
    }
}

fn format_bytes(size: u64) -> String {
    if size > 1024 * 1024 * 1024 {
        format!("{:.2} GB", size as f64 / (1024.0 * 1024.0 * 1024.0))
    } else if size > 1024 * 1024 {
        format!("{:.2} MB", size as f64 / (1024.0 * 1024.0))
    } else if size > 1024 {
        format!("{:.1} KB", size as f64 / 1024.0)
    } else {
        format!("{} B", size)
    }
}
