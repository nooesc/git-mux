use std::collections::{HashMap, HashSet};
use std::time::Instant;

use crate::github::contributions::ContributionData;
use crate::github::prs::PrState;
use crate::github::repos::RepoInfo;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrSection {
    Authored,
    ReviewRequested,
}

// ── App State ──

pub struct AppState {
    pub active_view: View,
    pub should_quit: bool,

    // Loading & error state
    pub loading: HashSet<View>,
    pub last_refresh: HashMap<View, Instant>,
    pub error: Option<String>,

    // Repo state
    pub repos: Vec<RepoInfo>,
    pub repo_selected: usize,

    // PR state
    pub prs: PrState,
    pub pr_selected: usize,
    pub pr_section: PrSection,

    // Contribution graph
    pub contributions: ContributionData,

    // Open-in-browser
    pub pending_open_url: Option<String>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            active_view: View::Repos,
            should_quit: false,
            loading: HashSet::new(),
            last_refresh: HashMap::new(),
            error: None,
            repos: Vec::new(),
            repo_selected: 0,
            prs: PrState::default(),
            pr_selected: 0,
            pr_section: PrSection::Authored,
            contributions: ContributionData::default(),
            pending_open_url: None,
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
    ReposLoaded(Vec<RepoInfo>),
    PrsLoaded(PrState),
    ContributionsLoaded(ContributionData),
    TogglePrSection,
}

// ── Update ──

pub fn update(state: &mut AppState, msg: Message) {
    match msg {
        Message::Quit => state.should_quit = true,
        Message::SwitchView(view) => state.active_view = view,
        Message::Error(e) => state.error = Some(e),
        Message::DismissError => state.error = None,
        Message::ReposLoaded(repos) => {
            state.repos = repos;
            state.loading.remove(&View::Repos);
            state.last_refresh.insert(View::Repos, Instant::now());
        }
        Message::PrsLoaded(prs) => {
            state.prs = prs;
            state.loading.remove(&View::PRs);
            state.last_refresh.insert(View::PRs, Instant::now());
        }
        Message::ContributionsLoaded(data) => {
            state.contributions = data;
            state.loading.remove(&View::Graph);
            state.last_refresh.insert(View::Graph, Instant::now());
        }
        Message::TogglePrSection => {
            state.pr_section = match state.pr_section {
                PrSection::Authored => PrSection::ReviewRequested,
                PrSection::ReviewRequested => PrSection::Authored,
            };
            state.pr_selected = 0;
        }
        Message::Up => {
            match state.active_view {
                View::Repos => {
                    if state.repo_selected > 0 {
                        state.repo_selected -= 1;
                    }
                }
                View::PRs => {
                    if state.pr_selected > 0 {
                        state.pr_selected -= 1;
                    }
                }
                _ => {}
            }
        }
        Message::Down => {
            match state.active_view {
                View::Repos => {
                    if state.repo_selected < state.repos.len().saturating_sub(1) {
                        state.repo_selected += 1;
                    }
                }
                View::PRs => {
                    let len = match state.pr_section {
                        PrSection::Authored => state.prs.authored.len(),
                        PrSection::ReviewRequested => state.prs.review_requested.len(),
                    };
                    if state.pr_selected < len.saturating_sub(1) {
                        state.pr_selected += 1;
                    }
                }
                _ => {}
            }
        }
        Message::Select => {
            match state.active_view {
                View::Repos => {
                    if let Some(repo) = state.repos.get(state.repo_selected) {
                        state.pending_open_url = Some(repo.html_url.clone());
                    }
                }
                View::PRs => {
                    let pr = match state.pr_section {
                        PrSection::Authored => state.prs.authored.get(state.pr_selected),
                        PrSection::ReviewRequested => state.prs.review_requested.get(state.pr_selected),
                    };
                    if let Some(pr) = pr {
                        state.pending_open_url = Some(pr.html_url.clone());
                    }
                }
                _ => {}
            }
        }
        Message::Back | Message::Tick | Message::ForceRefresh => {}
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

    #[test]
    fn test_repos_loaded() {
        let mut state = AppState::new();
        state.loading.insert(View::Repos);

        let repos = vec![RepoInfo {
            full_name: "user/test".to_string(),
            description: None,
            language: Some("Rust".to_string()),
            stargazers_count: 5,
            forks_count: 1,
            open_issues_count: 2,
            pushed_at: None,
            html_url: "https://github.com/user/test".to_string(),
            is_fork: false,
            is_private: false,
            owner: "user".to_string(),
        }];

        update(&mut state, Message::ReposLoaded(repos));
        assert_eq!(state.repos.len(), 1);
        assert!(!state.loading.contains(&View::Repos));
    }

    #[test]
    fn test_repo_navigation() {
        let mut state = AppState::new();
        state.repos = vec![
            RepoInfo {
                full_name: "a".to_string(), description: None, language: None,
                stargazers_count: 0, forks_count: 0, open_issues_count: 0,
                pushed_at: None, html_url: String::new(), is_fork: false,
                is_private: false, owner: String::new(),
            },
            RepoInfo {
                full_name: "b".to_string(), description: None, language: None,
                stargazers_count: 0, forks_count: 0, open_issues_count: 0,
                pushed_at: None, html_url: String::new(), is_fork: false,
                is_private: false, owner: String::new(),
            },
        ];

        update(&mut state, Message::Down);
        assert_eq!(state.repo_selected, 1);
        update(&mut state, Message::Down);
        assert_eq!(state.repo_selected, 1); // can't go past end
        update(&mut state, Message::Up);
        assert_eq!(state.repo_selected, 0);
        update(&mut state, Message::Up);
        assert_eq!(state.repo_selected, 0); // can't go below 0
    }
}
