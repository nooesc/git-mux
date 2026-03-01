use chrono::Utc;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::app::AppState;

pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
    // Center the overlay: 80% width, 70% height
    let popup_area = centered_rect(80, 70, area);

    // Clear underlying content
    frame.render_widget(Clear, popup_area);

    let filtered = state.filtered_notifications();
    let unread_count = state.unread_count();

    let title = format!(" Notifications ({} unread) ", unread_count);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(title);

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    if filtered.is_empty() {
        frame.render_widget(
            Paragraph::new("  No notifications").style(Style::default().fg(Color::DarkGray)),
            inner,
        );
        return;
    }

    let mut lines: Vec<Line> = Vec::new();
    for (idx, notif) in filtered.iter().enumerate() {
        let selected = idx == state.notif_selected;

        let unread_marker = if notif.unread { "●" } else { "○" };
        let marker_color = if notif.unread { Color::Yellow } else { Color::DarkGray };

        let type_label = match notif.subject_type.as_str() {
            "PullRequest" => "PR",
            "Issue" => "IS",
            "Release" => "RE",
            "CheckSuite" => "CI",
            "Discussion" => "DI",
            _ => "??",
        };
        let type_color = match notif.subject_type.as_str() {
            "PullRequest" => Color::Green,
            "Issue" => Color::Yellow,
            "Release" => Color::Magenta,
            "CheckSuite" => Color::Red,
            _ => Color::DarkGray,
        };

        let style = if selected {
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
        } else if notif.unread {
            Style::default().add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let age = notif.updated_at.map(|dt| {
            let ago = Utc::now().signed_duration_since(dt);
            if ago.num_hours() < 1 { format!("{}m ago", ago.num_minutes()) }
            else if ago.num_hours() < 24 { format!("{}h ago", ago.num_hours()) }
            else { format!("{}d ago", ago.num_days()) }
        }).unwrap_or_default();

        lines.push(Line::from(vec![
            Span::raw(if selected { "  > " } else { "    " }),
            Span::styled(unread_marker, Style::default().fg(marker_color)),
            Span::raw(" "),
            Span::styled(type_label, Style::default().fg(type_color).add_modifier(Modifier::BOLD)),
            Span::raw("  "),
            Span::styled(&notif.repo_full_name, Style::default().fg(Color::DarkGray)),
        ]));
        lines.push(Line::from(vec![
            Span::raw("         "),
            Span::styled(&notif.subject_title, style),
            Span::raw("  "),
            Span::styled(age, Style::default().fg(Color::DarkGray)),
        ]));
        lines.push(Line::from(""));
    }

    // Hint bar at bottom
    let hints = Line::from(vec![
        Span::styled(
            " j/k nav · Enter open · m mark read · a mark all · Esc close ",
            Style::default().fg(Color::DarkGray),
        ),
    ]);

    // Split: content + hint
    let [content_area, hint_area] = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(1),
    ]).areas(inner);

    // Scroll
    let item_height = 3;
    let selected_top = state.notif_selected * item_height;
    let visible = content_area.height as usize;
    let scroll = if selected_top + item_height > visible { selected_top + item_height - visible } else { 0 };

    let visible_lines: Vec<Line> = lines.into_iter().skip(scroll).take(visible).collect();
    frame.render_widget(Paragraph::new(visible_lines), content_area);
    frame.render_widget(Paragraph::new(hints), hint_area);
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let [_, center_v, _] = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ]).areas(area);
    let [_, center, _] = Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ]).areas(center_v);
    center
}
