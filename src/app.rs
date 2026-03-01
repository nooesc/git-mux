use std::collections::HashSet;
use std::time::Instant;

use image::DynamicImage;

use crate::github::ci::WorkflowRun;
use crate::github::contributions::ContributionData;
use crate::github::issues::IssueInfo;
use crate::github::notifications::Notification;
use crate::github::prs::PrInfo;
use crate::github::repos::RepoInfo;
use crate::github::UserInfo;

// ── Screen hierarchy ──

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Screen {
    Home,
    RepoDetail {
        repo_full_name: String,
        section: RepoSection,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepoSection {
    PRs,
    Issues,
    CI,
}

impl RepoSection {
    pub fn next(self) -> Self {
        match self {
            Self::PRs => Self::Issues,
            Self::Issues => Self::CI,
            Self::CI => Self::PRs,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::PRs => "PRs",
            Self::Issues => "Issues",
            Self::CI => "CI",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    Cards,
    List,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HomeFocus {
    FilterBar,
    Repos,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepoFilter {
    All,
    Public,
    Private,
    Org(String),
}

// ── App State ──

pub struct AppState {
    // Navigation
    pub screen: Screen,
    pub should_quit: bool,

    // Home screen data
    pub repos: Vec<RepoInfo>,
    pub contributions: ContributionData,
    pub avatar: Option<DynamicImage>,
    pub user_info: Option<UserInfo>,

    // Home screen UI state
    pub card_selected: usize,
    pub view_mode: ViewMode,
    pub home_scroll: u16,
    pub home_focus: HomeFocus,
    pub repo_filter: RepoFilter,
    pub filter_index: usize,

    // Config (for exclusions)
    pub exclude_orgs: Vec<String>,
    pub exclude_repos: Vec<String>,

    // Repo detail data (lazy-loaded when entering a repo)
    pub repo_prs: Vec<PrInfo>,
    pub repo_issues: Vec<IssueInfo>,
    pub repo_ci: Vec<WorkflowRun>,
    pub detail_selected: usize,

    // Notifications (global, shown as overlay)
    pub notifications: Vec<Notification>,
    pub notif_selected: usize,
    pub show_notifications: bool,

    // Loading state (string keys: "repos", "contributions", "avatar", "notifications", "repo_detail")
    pub loading: HashSet<String>,

    // Error
    pub error: Option<String>,
    pub error_at: Option<Instant>,

    // UI overlays
    pub show_help: bool,
    pub search_mode: bool,
    pub search_query: String,

    // Open-in-browser
    pub pending_open_url: Option<String>,

    // Terminal dimensions (updated on resize)
    pub term_width: u16,
    pub term_height: u16,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            screen: Screen::Home,
            should_quit: false,
            repos: Vec::new(),
            contributions: ContributionData::default(),
            avatar: None,
            user_info: None,
            card_selected: 0,
            view_mode: ViewMode::Cards,
            home_scroll: 0,
            home_focus: HomeFocus::Repos,
            repo_filter: RepoFilter::All,
            filter_index: 0,
            exclude_orgs: Vec::new(),
            exclude_repos: Vec::new(),
            repo_prs: Vec::new(),
            repo_issues: Vec::new(),
            repo_ci: Vec::new(),
            detail_selected: 0,
            notifications: Vec::new(),
            notif_selected: 0,
            show_notifications: false,
            loading: HashSet::new(),
            error: None,
            error_at: None,
            show_help: false,
            search_mode: false,
            search_query: String::new(),
            pending_open_url: None,
            term_width: 120,
            term_height: 40,
        }
    }

    /// Number of card columns based on terminal width.
    pub fn num_card_cols(&self) -> usize {
        if self.term_width >= 120 { 3 }
        else if self.term_width >= 80 { 2 }
        else { 1 }
    }

    /// Build the list of filter options from loaded repos.
    pub fn filter_options(&self) -> Vec<RepoFilter> {
        let mut opts = vec![RepoFilter::All, RepoFilter::Public, RepoFilter::Private];
        let username = self.user_info.as_ref().map(|u| u.login.as_str()).unwrap_or("");
        let mut orgs: Vec<&str> = self.repos.iter()
            .map(|r| r.owner.as_str())
            .filter(|o| !o.is_empty() && *o != username)
            .collect();
        orgs.sort_unstable();
        orgs.dedup();
        for org in orgs {
            opts.push(RepoFilter::Org(org.to_string()));
        }
        opts
    }

    /// Get repos filtered by search query and repo filter.
    pub fn filtered_repos(&self) -> Vec<&RepoInfo> {
        let filter_iter = self.repos.iter().filter(|r| {
            match &self.repo_filter {
                RepoFilter::All => true,
                RepoFilter::Public => !r.is_private,
                RepoFilter::Private => r.is_private,
                RepoFilter::Org(name) => r.owner == *name,
            }
        });

        if self.search_query.is_empty() {
            return filter_iter.collect();
        }
        let q = self.search_query.to_lowercase();
        filter_iter.filter(|r| {
            r.full_name.to_lowercase().contains(&q)
                || r.description.as_ref().is_some_and(|d| d.to_lowercase().contains(&q))
                || r.language.as_ref().is_some_and(|l| l.to_lowercase().contains(&q))
        }).collect()
    }

    /// Get the currently selected repo (if any), accounting for search filter.
    pub fn selected_repo(&self) -> Option<&RepoInfo> {
        self.filtered_repos().get(self.card_selected).copied()
    }

    /// Get repo detail items filtered by search query.
    pub fn filtered_detail_items(&self) -> Vec<DetailItem> {
        let items: Vec<DetailItem> = match &self.screen {
            Screen::RepoDetail { section: RepoSection::PRs, .. } => {
                self.repo_prs.iter().map(|pr| DetailItem::Pr(pr)).collect()
            }
            Screen::RepoDetail { section: RepoSection::Issues, .. } => {
                self.repo_issues.iter().map(|i| DetailItem::Issue(i)).collect()
            }
            Screen::RepoDetail { section: RepoSection::CI, .. } => {
                self.repo_ci.iter().map(|r| DetailItem::Ci(r)).collect()
            }
            _ => Vec::new(),
        };

        if self.search_query.is_empty() {
            return items;
        }
        let q = self.search_query.to_lowercase();
        items.into_iter().filter(|item| {
            match item {
                DetailItem::Pr(pr) => pr.title.to_lowercase().contains(&q) || pr.user.to_lowercase().contains(&q),
                DetailItem::Issue(i) => i.title.to_lowercase().contains(&q) || i.user.to_lowercase().contains(&q),
                DetailItem::Ci(r) => r.name.to_lowercase().contains(&q) || r.head_branch.to_lowercase().contains(&q),
            }
        }).collect()
    }

    /// Filtered notifications.
    pub fn filtered_notifications(&self) -> Vec<&Notification> {
        if self.search_query.is_empty() {
            return self.notifications.iter().collect();
        }
        let q = self.search_query.to_lowercase();
        self.notifications.iter().filter(|n| {
            n.subject_title.to_lowercase().contains(&q)
                || n.repo_full_name.to_lowercase().contains(&q)
        }).collect()
    }

    /// Count of unread notifications.
    pub fn unread_count(&self) -> usize {
        self.notifications.iter().filter(|n| n.unread).count()
    }

    /// Breadcrumb path for header bar.
    pub fn breadcrumb(&self) -> String {
        match &self.screen {
            Screen::Home => "Home".to_string(),
            Screen::RepoDetail { repo_full_name, section } => {
                format!("Home > {} > {}", repo_full_name, section.label())
            }
        }
    }
}

/// Wrapper for detail view items to allow mixed lists.
pub enum DetailItem<'a> {
    Pr(&'a PrInfo),
    Issue(&'a IssueInfo),
    Ci(&'a WorkflowRun),
}

// ── Messages ──

#[derive(Debug)]
pub enum Message {
    // Navigation
    Quit,
    Up,
    Down,
    Left,
    Right,
    Select,
    Back,
    GoHome,
    CycleSection,
    ToggleViewMode,

    // Data loaded
    ReposLoaded(Vec<RepoInfo>),
    ContributionsLoaded(ContributionData),
    AvatarLoaded(Vec<u8>),
    UserInfoLoaded(UserInfo),
    NotificationsLoaded(Vec<Notification>),
    RepoDetailLoaded {
        repo: String,
        prs: Vec<PrInfo>,
        issues: Vec<IssueInfo>,
        ci: Vec<WorkflowRun>,
    },

    // Actions
    ToggleNotifications,
    MarkNotifRead(String),
    MarkAllNotifsRead,
    ForceRefresh,

    // UI
    Tick,
    Error(String),
    DismissError,
    ToggleHelp,
    EnterSearch,
    ExitSearch,
    SearchInput(char),
    SearchBackspace,
    Resize(u16, u16),
}

// ── Update ──

pub fn update(state: &mut AppState, msg: Message) {
    match msg {
        Message::Quit => state.should_quit = true,

        Message::Error(e) => {
            state.error = Some(e);
            state.error_at = Some(Instant::now());
        }
        Message::DismissError => {
            state.error = None;
            state.error_at = None;
        }

        Message::Resize(w, h) => {
            state.term_width = w;
            state.term_height = h;
        }

        // ── Data loaded ──

        Message::ReposLoaded(repos) => {
            state.repos = repos.into_iter().filter(|r| {
                !state.exclude_orgs.iter().any(|o| o.eq_ignore_ascii_case(&r.owner))
                    && !state.exclude_repos.iter().any(|e| e.eq_ignore_ascii_case(&r.full_name))
            }).collect();
            state.loading.remove("repos");
        }
        Message::ContributionsLoaded(data) => {
            state.contributions = data;
            state.loading.remove("contributions");
        }
        Message::AvatarLoaded(bytes) => {
            state.avatar = image::load_from_memory(&bytes).ok();
            state.loading.remove("avatar");
        }
        Message::UserInfoLoaded(info) => {
            state.user_info = Some(info);
        }
        Message::NotificationsLoaded(notifs) => {
            state.notifications = notifs;
            state.loading.remove("notifications");
        }
        Message::RepoDetailLoaded { repo, prs, issues, ci } => {
            // Only apply if we're still on the same repo
            if let Screen::RepoDetail { repo_full_name, .. } = &state.screen {
                if *repo_full_name == repo {
                    state.repo_prs = prs;
                    state.repo_issues = issues;
                    state.repo_ci = ci;
                    state.loading.remove("repo_detail");
                }
            }
        }

        // ── Navigation ──

        Message::Up => {
            if state.show_notifications {
                if state.notif_selected > 0 {
                    state.notif_selected -= 1;
                }
            } else {
                match &state.screen {
                    Screen::Home => {
                        if state.home_focus == HomeFocus::FilterBar {
                            // Already at top, do nothing
                        } else {
                            let at_top = match state.view_mode {
                                ViewMode::Cards => state.card_selected < state.num_card_cols(),
                                ViewMode::List => state.card_selected == 0,
                            };
                            if at_top {
                                state.home_focus = HomeFocus::FilterBar;
                            } else {
                                match state.view_mode {
                                    ViewMode::Cards => {
                                        let cols = state.num_card_cols();
                                        if state.card_selected >= cols {
                                            state.card_selected -= cols;
                                        }
                                    }
                                    ViewMode::List => {
                                        if state.card_selected > 0 {
                                            state.card_selected -= 1;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Screen::RepoDetail { .. } => {
                        if state.detail_selected > 0 {
                            state.detail_selected -= 1;
                        }
                    }
                }
            }
        }

        Message::Down => {
            if state.show_notifications {
                let len = state.filtered_notifications().len();
                if state.notif_selected < len.saturating_sub(1) {
                    state.notif_selected += 1;
                }
            } else {
                match &state.screen {
                    Screen::Home => {
                        if state.home_focus == HomeFocus::FilterBar {
                            state.home_focus = HomeFocus::Repos;
                        } else {
                            let cols = state.num_card_cols();
                            let len = state.filtered_repos().len();
                            match state.view_mode {
                                ViewMode::Cards => {
                                    if state.card_selected + cols < len {
                                        state.card_selected += cols;
                                    }
                                }
                                ViewMode::List => {
                                    if state.card_selected < len.saturating_sub(1) {
                                        state.card_selected += 1;
                                    }
                                }
                            }
                        }
                    }
                    Screen::RepoDetail { .. } => {
                        let len = state.filtered_detail_items().len();
                        if state.detail_selected < len.saturating_sub(1) {
                            state.detail_selected += 1;
                        }
                    }
                }
            }
        }

        Message::Left => {
            if state.screen == Screen::Home {
                if state.home_focus == HomeFocus::FilterBar {
                    if state.filter_index > 0 {
                        state.filter_index -= 1;
                        let opts = state.filter_options();
                        state.repo_filter = opts[state.filter_index].clone();
                        state.card_selected = 0;
                    }
                } else if state.view_mode == ViewMode::Cards && state.card_selected > 0 {
                    state.card_selected -= 1;
                }
            }
        }

        Message::Right => {
            if state.screen == Screen::Home {
                if state.home_focus == HomeFocus::FilterBar {
                    let opts = state.filter_options();
                    if state.filter_index + 1 < opts.len() {
                        state.filter_index += 1;
                        state.repo_filter = opts[state.filter_index].clone();
                        state.card_selected = 0;
                    }
                } else if state.view_mode == ViewMode::Cards {
                    let len = state.filtered_repos().len();
                    if state.card_selected + 1 < len {
                        state.card_selected += 1;
                    }
                }
            }
        }

        Message::Select => {
            if state.show_notifications {
                // Open notification URL in browser
                let filtered = state.filtered_notifications();
                if let Some(notif) = filtered.get(state.notif_selected) {
                    if let Some(ref url) = notif.url {
                        state.pending_open_url = Some(url.clone());
                    }
                }
            } else {
                match &state.screen {
                    Screen::Home => {
                        if state.home_focus == HomeFocus::FilterBar {
                            state.home_focus = HomeFocus::Repos;
                        } else {
                            // Drill into repo detail (keep existing code)
                            let filtered = state.filtered_repos();
                            if let Some(repo) = filtered.get(state.card_selected) {
                                let repo_full_name = repo.full_name.clone();
                                state.screen = Screen::RepoDetail {
                                    repo_full_name,
                                    section: RepoSection::PRs,
                                };
                                state.detail_selected = 0;
                                state.repo_prs.clear();
                                state.repo_issues.clear();
                                state.repo_ci.clear();
                                state.loading.insert("repo_detail".to_string());
                                state.search_query.clear();
                                state.search_mode = false;
                            }
                        }
                    }
                    Screen::RepoDetail { .. } => {
                        // Open item in browser
                        let items = state.filtered_detail_items();
                        if let Some(item) = items.get(state.detail_selected) {
                            let url = match item {
                                DetailItem::Pr(pr) => &pr.html_url,
                                DetailItem::Issue(i) => &i.html_url,
                                DetailItem::Ci(r) => &r.html_url,
                            };
                            state.pending_open_url = Some(url.clone());
                        }
                    }
                }
            }
        }

        Message::Back => {
            if state.show_notifications {
                state.show_notifications = false;
            } else if state.search_mode {
                state.search_mode = false;
                state.search_query.clear();
            } else {
                match &state.screen {
                    Screen::RepoDetail { .. } => {
                        state.screen = Screen::Home;
                        state.detail_selected = 0;
                        state.search_query.clear();
                    }
                    Screen::Home => {} // already at root
                }
            }
        }

        Message::GoHome => {
            state.screen = Screen::Home;
            state.show_notifications = false;
            state.search_mode = false;
            state.search_query.clear();
        }

        Message::CycleSection => {
            if let Screen::RepoDetail { section, .. } = &mut state.screen {
                *section = section.next();
                state.detail_selected = 0;
            }
        }

        Message::ToggleViewMode => {
            if state.screen == Screen::Home {
                state.view_mode = match state.view_mode {
                    ViewMode::Cards => ViewMode::List,
                    ViewMode::List => ViewMode::Cards,
                };
            }
        }

        // ── Actions ──

        Message::ToggleNotifications => {
            state.show_notifications = !state.show_notifications;
            if state.show_notifications {
                state.notif_selected = 0;
            }
        }

        Message::MarkNotifRead(thread_id) => {
            if let Some(notif) = state.notifications.iter_mut().find(|n| n.id == thread_id) {
                notif.unread = false;
            }
        }

        Message::MarkAllNotifsRead => {
            for notif in &mut state.notifications {
                notif.unread = false;
            }
        }

        // ── UI ──

        Message::ToggleHelp => state.show_help = !state.show_help,
        Message::EnterSearch => state.search_mode = true,
        Message::ExitSearch => {
            state.search_mode = false;
            state.search_query.clear();
        }
        Message::SearchInput(c) => {
            state.search_query.push(c);
            // Reset selection when search changes
            state.card_selected = 0;
            state.detail_selected = 0;
            state.notif_selected = 0;
        }
        Message::SearchBackspace => {
            state.search_query.pop();
            state.card_selected = 0;
            state.detail_selected = 0;
            state.notif_selected = 0;
        }

        Message::Tick => {
            // Auto-dismiss error after 10 seconds
            if let Some(at) = state.error_at {
                if at.elapsed().as_secs() > 10 {
                    state.error = None;
                    state.error_at = None;
                }
            }
        }

        Message::ForceRefresh => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_state() {
        let state = AppState::new();
        assert_eq!(state.screen, Screen::Home);
        assert!(!state.should_quit);
        assert!(state.error.is_none());
    }

    #[test]
    fn test_quit() {
        let mut state = AppState::new();
        update(&mut state, Message::Quit);
        assert!(state.should_quit);
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
    fn test_repos_loaded() {
        let mut state = AppState::new();
        state.loading.insert("repos".to_string());

        let repos = vec![make_repo("user/test")];
        update(&mut state, Message::ReposLoaded(repos));
        assert_eq!(state.repos.len(), 1);
        assert!(!state.loading.contains("repos"));
    }

    #[test]
    fn test_card_navigation_list_mode() {
        let mut state = AppState::new();
        state.view_mode = ViewMode::List;
        state.repos = vec![make_repo("a"), make_repo("b"), make_repo("c")];

        update(&mut state, Message::Down);
        assert_eq!(state.card_selected, 1);
        update(&mut state, Message::Down);
        assert_eq!(state.card_selected, 2);
        update(&mut state, Message::Down);
        assert_eq!(state.card_selected, 2); // can't pass end
        update(&mut state, Message::Up);
        assert_eq!(state.card_selected, 1);
    }

    #[test]
    fn test_card_navigation_grid_mode() {
        let mut state = AppState::new();
        state.view_mode = ViewMode::Cards;
        state.term_width = 120; // 3 columns
        state.repos = vec![
            make_repo("a"), make_repo("b"), make_repo("c"),
            make_repo("d"), make_repo("e"), make_repo("f"),
        ];

        // Down jumps by 3 (one row)
        update(&mut state, Message::Down);
        assert_eq!(state.card_selected, 3);

        // Right moves by 1
        update(&mut state, Message::Right);
        assert_eq!(state.card_selected, 4);

        // Up jumps back by 3
        update(&mut state, Message::Up);
        assert_eq!(state.card_selected, 1);

        // Left moves by 1
        update(&mut state, Message::Left);
        assert_eq!(state.card_selected, 0);
    }

    #[test]
    fn test_drill_into_repo() {
        let mut state = AppState::new();
        state.repos = vec![make_repo("user/myrepo")];

        update(&mut state, Message::Select);
        assert_eq!(state.screen, Screen::RepoDetail {
            repo_full_name: "user/myrepo".to_string(),
            section: RepoSection::PRs,
        });
        assert!(state.loading.contains("repo_detail"));
    }

    #[test]
    fn test_back_from_repo_detail() {
        let mut state = AppState::new();
        state.screen = Screen::RepoDetail {
            repo_full_name: "user/repo".to_string(),
            section: RepoSection::PRs,
        };

        update(&mut state, Message::Back);
        assert_eq!(state.screen, Screen::Home);
    }

    #[test]
    fn test_cycle_section() {
        let mut state = AppState::new();
        state.screen = Screen::RepoDetail {
            repo_full_name: "user/repo".to_string(),
            section: RepoSection::PRs,
        };

        update(&mut state, Message::CycleSection);
        assert!(matches!(state.screen, Screen::RepoDetail { section: RepoSection::Issues, .. }));

        update(&mut state, Message::CycleSection);
        assert!(matches!(state.screen, Screen::RepoDetail { section: RepoSection::CI, .. }));

        update(&mut state, Message::CycleSection);
        assert!(matches!(state.screen, Screen::RepoDetail { section: RepoSection::PRs, .. }));
    }

    #[test]
    fn test_toggle_view_mode() {
        let mut state = AppState::new();
        assert_eq!(state.view_mode, ViewMode::Cards);
        update(&mut state, Message::ToggleViewMode);
        assert_eq!(state.view_mode, ViewMode::List);
        update(&mut state, Message::ToggleViewMode);
        assert_eq!(state.view_mode, ViewMode::Cards);
    }

    #[test]
    fn test_notification_overlay() {
        let mut state = AppState::new();
        assert!(!state.show_notifications);
        update(&mut state, Message::ToggleNotifications);
        assert!(state.show_notifications);
        update(&mut state, Message::Back);
        assert!(!state.show_notifications);
    }

    #[test]
    fn test_mark_notif_read() {
        let mut state = AppState::new();
        state.notifications = vec![make_notification("1", true), make_notification("2", true)];

        update(&mut state, Message::MarkNotifRead("1".to_string()));
        assert!(!state.notifications[0].unread);
        assert!(state.notifications[1].unread);
    }

    #[test]
    fn test_mark_all_notifs_read() {
        let mut state = AppState::new();
        state.notifications = vec![make_notification("1", true), make_notification("2", true)];

        update(&mut state, Message::MarkAllNotifsRead);
        assert!(!state.notifications[0].unread);
        assert!(!state.notifications[1].unread);
    }

    #[test]
    fn test_go_home() {
        let mut state = AppState::new();
        state.screen = Screen::RepoDetail {
            repo_full_name: "x/y".to_string(),
            section: RepoSection::CI,
        };
        state.show_notifications = true;
        state.search_mode = true;

        update(&mut state, Message::GoHome);
        assert_eq!(state.screen, Screen::Home);
        assert!(!state.show_notifications);
        assert!(!state.search_mode);
    }

    #[test]
    fn test_search_resets_selection() {
        let mut state = AppState::new();
        state.card_selected = 5;
        update(&mut state, Message::SearchInput('a'));
        assert_eq!(state.card_selected, 0);
        assert_eq!(state.search_query, "a");
    }

    #[test]
    fn test_breadcrumb() {
        let mut state = AppState::new();
        assert_eq!(state.breadcrumb(), "Home");

        state.screen = Screen::RepoDetail {
            repo_full_name: "nooesc/ghd".to_string(),
            section: RepoSection::PRs,
        };
        assert_eq!(state.breadcrumb(), "Home > nooesc/ghd > PRs");
    }

    #[test]
    fn test_unread_count() {
        let mut state = AppState::new();
        state.notifications = vec![
            make_notification("1", true),
            make_notification("2", false),
            make_notification("3", true),
        ];
        assert_eq!(state.unread_count(), 2);
    }

    #[test]
    fn test_resize() {
        let mut state = AppState::new();
        update(&mut state, Message::Resize(80, 24));
        assert_eq!(state.term_width, 80);
        assert_eq!(state.term_height, 24);
        assert_eq!(state.num_card_cols(), 2);
    }

    #[test]
    fn test_initial_home_focus() {
        let state = AppState::new();
        assert_eq!(state.home_focus, HomeFocus::Repos);
        assert_eq!(state.repo_filter, RepoFilter::All);
        assert_eq!(state.filter_index, 0);
    }

    #[test]
    fn test_filter_options_generation() {
        let mut state = AppState::new();
        state.repos = vec![
            make_repo("alice/foo"),
            make_repo_private("alice/secret"),
            make_repo("acme-corp/tool"),
        ];
        state.user_info = Some(crate::github::UserInfo {
            login: "alice".to_string(),
            avatar_url: String::new(),
            public_repos: 2,
            followers: 0,
        });
        let opts = state.filter_options();
        assert_eq!(opts.len(), 4);
        assert_eq!(opts[0], RepoFilter::All);
        assert_eq!(opts[1], RepoFilter::Public);
        assert_eq!(opts[2], RepoFilter::Private);
        assert_eq!(opts[3], RepoFilter::Org("acme-corp".to_string()));
    }

    #[test]
    fn test_filtered_repos_by_filter() {
        let mut state = AppState::new();
        state.repos = vec![
            make_repo("alice/public1"),
            make_repo_private("alice/secret"),
            make_repo("acme-corp/tool"),
        ];

        state.repo_filter = RepoFilter::All;
        assert_eq!(state.filtered_repos().len(), 3);

        state.repo_filter = RepoFilter::Public;
        assert_eq!(state.filtered_repos().len(), 2);

        state.repo_filter = RepoFilter::Private;
        assert_eq!(state.filtered_repos().len(), 1);

        state.repo_filter = RepoFilter::Org("acme-corp".to_string());
        assert_eq!(state.filtered_repos().len(), 1);
    }

    #[test]
    fn test_config_exclusions() {
        let mut state = AppState::new();
        state.exclude_orgs = vec!["boring-corp".to_string()];
        state.exclude_repos = vec!["alice/junk".to_string()];
        state.loading.insert("repos".to_string());

        let repos = vec![
            make_repo("alice/good"),
            make_repo("alice/junk"),
            make_repo("boring-corp/tool"),
            make_repo("acme/nice"),
        ];
        update(&mut state, Message::ReposLoaded(repos));

        assert_eq!(state.repos.len(), 2);
        assert_eq!(state.repos[0].full_name, "alice/good");
        assert_eq!(state.repos[1].full_name, "acme/nice");
    }

    #[test]
    fn test_up_from_first_repo_moves_to_filter_bar() {
        let mut state = AppState::new();
        state.home_focus = HomeFocus::Repos;
        state.card_selected = 0;
        state.view_mode = ViewMode::List;
        state.repos = vec![make_repo("a/b")];

        update(&mut state, Message::Up);
        assert_eq!(state.home_focus, HomeFocus::FilterBar);
    }

    #[test]
    fn test_down_from_filter_bar_moves_to_repos() {
        let mut state = AppState::new();
        state.home_focus = HomeFocus::FilterBar;
        state.repos = vec![make_repo("a/b")];

        update(&mut state, Message::Down);
        assert_eq!(state.home_focus, HomeFocus::Repos);
    }

    #[test]
    fn test_left_right_on_filter_bar() {
        let mut state = AppState::new();
        state.home_focus = HomeFocus::FilterBar;
        state.filter_index = 0;
        state.repos = vec![make_repo("alice/foo"), make_repo("acme/bar")];
        state.user_info = Some(crate::github::UserInfo {
            login: "alice".to_string(),
            avatar_url: String::new(),
            public_repos: 1,
            followers: 0,
        });
        // filter_options: All, Public, Private, acme
        update(&mut state, Message::Right);
        assert_eq!(state.filter_index, 1);
        assert_eq!(state.repo_filter, RepoFilter::Public);

        update(&mut state, Message::Right);
        assert_eq!(state.filter_index, 2);
        assert_eq!(state.repo_filter, RepoFilter::Private);

        update(&mut state, Message::Left);
        assert_eq!(state.filter_index, 1);
        assert_eq!(state.repo_filter, RepoFilter::Public);
    }

    #[test]
    fn test_select_on_filter_bar_drops_to_repos() {
        let mut state = AppState::new();
        state.home_focus = HomeFocus::FilterBar;
        state.repos = vec![make_repo("a/b")];

        update(&mut state, Message::Select);
        assert_eq!(state.home_focus, HomeFocus::Repos);
    }

    // ── Test helpers ──

    fn make_repo_private(name: &str) -> RepoInfo {
        let mut repo = make_repo(name);
        repo.is_private = true;
        repo
    }

    fn make_repo(name: &str) -> RepoInfo {
        let owner = name.split('/').next().unwrap_or("").to_string();
        RepoInfo {
            full_name: name.to_string(),
            description: None,
            language: None,
            stargazers_count: 0,
            forks_count: 0,
            open_issues_count: 0,
            pushed_at: None,
            html_url: format!("https://github.com/{}", name),
            is_fork: false,
            is_private: false,
            owner,
        }
    }

    fn make_notification(id: &str, unread: bool) -> Notification {
        Notification {
            id: id.to_string(),
            reason: "subscribed".to_string(),
            subject_title: "Test".to_string(),
            subject_type: "PullRequest".to_string(),
            repo_full_name: "user/repo".to_string(),
            updated_at: None,
            unread,
            url: Some("https://github.com/user/repo/pull/1".to_string()),
        }
    }
}
