pub mod header;
pub mod home;
pub mod notification_overlay;
pub mod repo_detail;

use ratatui::Frame;
use ratatui::layout::Rect;

use crate::app::{AppState, Screen};

pub fn render_content(frame: &mut Frame, area: Rect, state: &AppState) {
    match &state.screen {
        Screen::Home => home::render(frame, area, state),
        Screen::RepoDetail { .. } => repo_detail::render(frame, area, state),
    }
}
