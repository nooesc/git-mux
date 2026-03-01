use std::collections::{HashMap, HashSet};
use std::time::Instant;

// ── View enum ──

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum View {
    Repos,
    PRs,
    Graph,
    Notifications,
    CI,
}

impl View {
    pub const ALL: [View; 5] = [
        View::Repos,
        View::PRs,
        View::Graph,
        View::Notifications,
        View::CI,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            View::Repos => "Repos",
            View::PRs => "PRs",
            View::Graph => "Graph",
            View::Notifications => "Notifs",
            View::CI => "CI",
        }
    }

    pub fn index(&self) -> usize {
        match self {
            View::Repos => 0,
            View::PRs => 1,
            View::Graph => 2,
            View::Notifications => 3,
            View::CI => 4,
        }
    }
}

// ── App State ──

pub struct AppState {
    pub active_view: View,
    pub should_quit: bool,

    // Loading & error state
    pub loading: HashSet<View>,
    pub last_refresh: HashMap<View, Instant>,
    pub error: Option<String>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            active_view: View::Repos,
            should_quit: false,
            loading: HashSet::new(),
            last_refresh: HashMap::new(),
            error: None,
        }
    }
}

// ── Messages ──

#[derive(Debug)]
pub enum Message {
    SwitchView(View),
    Quit,
    Up,
    Down,
    Select,
    Back,
    Tick,
    ForceRefresh,
    Error(String),
    DismissError,
}

// ── Update ──

pub fn update(state: &mut AppState, msg: Message) {
    match msg {
        Message::Quit => state.should_quit = true,
        Message::SwitchView(view) => state.active_view = view,
        Message::Error(e) => state.error = Some(e),
        Message::DismissError => state.error = None,
        // Navigation and data messages will be handled as we add views
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_state() {
        let state = AppState::new();
        assert_eq!(state.active_view, View::Repos);
        assert!(!state.should_quit);
        assert!(state.error.is_none());
        assert!(state.loading.is_empty());
    }

    #[test]
    fn test_quit_message() {
        let mut state = AppState::new();
        update(&mut state, Message::Quit);
        assert!(state.should_quit);
    }

    #[test]
    fn test_switch_view() {
        let mut state = AppState::new();
        update(&mut state, Message::SwitchView(View::PRs));
        assert_eq!(state.active_view, View::PRs);
    }

    #[test]
    fn test_error_and_dismiss() {
        let mut state = AppState::new();
        update(&mut state, Message::Error("oops".into()));
        assert_eq!(state.error.as_deref(), Some("oops"));
        update(&mut state, Message::DismissError);
        assert!(state.error.is_none());
    }

    #[test]
    fn test_view_labels() {
        assert_eq!(View::Repos.label(), "Repos");
        assert_eq!(View::PRs.label(), "PRs");
        assert_eq!(View::CI.label(), "CI");
    }

    #[test]
    fn test_view_indices() {
        for (i, view) in View::ALL.iter().enumerate() {
            assert_eq!(view.index(), i);
        }
    }
}
