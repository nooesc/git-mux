pub mod repos;

use ratatui::Frame;
use ratatui::layout::Rect;
use crate::app::{AppState, View};

pub fn render_content(frame: &mut Frame, area: Rect, state: &AppState) {
    match state.active_view {
        View::Repos => repos::render(frame, area, state),
        _ => {
            let placeholder = ratatui::widgets::Paragraph::new(
                format!("{} (coming soon)", state.active_view.label())
            )
            .alignment(ratatui::layout::Alignment::Center)
            .block(ratatui::widgets::Block::default().borders(ratatui::widgets::Borders::ALL));
            frame.render_widget(placeholder, area);
        }
    }
}
