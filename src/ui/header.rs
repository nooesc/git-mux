use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::AppState;

pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
    let breadcrumb = state.breadcrumb();
    let unread = state.unread_count();

    let badge = if unread > 0 {
        Span::styled(
            format!("● {} ", unread),
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled("  ", Style::default().fg(Color::DarkGray))
    };

    // Calculate padding to right-align the badge
    let left_content = format!(" ghd   {} ", breadcrumb);
    let badge_width = if unread > 0 { format!("● {} ", unread).len() } else { 2 };
    let padding = (area.width as usize).saturating_sub(left_content.len() + badge_width);

    let line = Line::from(vec![
        Span::styled(" ghd", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::styled("   ", Style::default()),
        Span::styled(breadcrumb, Style::default().fg(Color::DarkGray)),
        Span::raw(" ".repeat(padding)),
        badge,
    ]);

    frame.render_widget(Paragraph::new(line), area);
}
