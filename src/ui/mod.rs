pub mod ci;
pub mod contributions;
pub mod notifications;
pub mod prs;
pub mod repos;

use ratatui::Frame;
use ratatui::layout::Rect;
use crate::app::{AppState, View};

pub fn render_content(frame: &mut Frame, area: Rect, state: &AppState) {
    match state.active_view {
        View::Repos => repos::render(frame, area, state),
        View::PRs => prs::render(frame, area, state),
        View::Graph => contributions::render(frame, area, state),
        View::Notifications => notifications::render(frame, area, state),
        View::CI => ci::render(frame, area, state),
    }
}
