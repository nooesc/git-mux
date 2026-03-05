use std::collections::{HashMap, HashSet};
use std::time::Instant;

use chrono::{DateTime, Utc};
use image::DynamicImage;

use crate::github::UserInfo;
use crate::github::ci::WorkflowRun;
use crate::github::commits::{CommitInfo, WeeklyCommitActivity};
use crate::github::contributions::ContributionData;
use crate::github::contributors::ContributorInfo;
use crate::github::issues::IssueInfo;
use crate::github::notifications::Notification;
use crate::github::prs::PrInfo;
use crate::github::repos::{RepoInfo, RepoOpenCounts};

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
    Commits,
    Info,
}

impl RepoSection {
    pub fn next(self) -> Self {
        match self {
            Self::PRs => Self::Issues,
            Self::Issues => Self::CI,
            Self::CI => Self::Commits,
            Self::Commits => Self::Info,
            Self::Info => Self::PRs,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Self::PRs => Self::Info,
            Self::Issues => Self::PRs,
            Self::CI => Self::Issues,
            Self::Commits => Self::CI,
            Self::Info => Self::Commits,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::PRs => "PRs",
            Self::Issues => "Issues",
            Self::CI => "CI",
            Self::Commits => "Commits",
            Self::Info => "Info",
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetailFocus {
    TabBar,
    Content,
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
    pub home_focus: HomeFocus,
    pub repo_filter: RepoFilter,
    pub filter_index: usize,

    // Config (for exclusions)
    pub exclude_orgs: Vec<String>,
    pub exclude_repos: Vec<String>,

    // Repo detail state
    pub detail_focus: DetailFocus,

    // Repo detail data (lazy-loaded when entering a repo)
    pub repo_prs: Vec<PrInfo>,
    pub repo_issues: Vec<IssueInfo>,
    pub repo_ci: Vec<WorkflowRun>,
    pub detail_selected: usize,
    pub repo_commits: Vec<CommitInfo>,
    pub repo_commit_activity: Vec<WeeklyCommitActivity>,
    pub repo_readme: Option<String>,
    pub repo_languages: Vec<(String, u64)>,
    pub repo_contributors: Vec<ContributorInfo>,
    pub repo_code_frequency: Vec<(i64, i64, i64)>,
    pub detail_cache_saved_at: Option<DateTime<Utc>>,

    // Notifications (global, shown as overlay)
    pub notifications: Vec<Notification>,
    pub notif_selected: usize,
    pub show_notifications: bool,

    // Loading state (string keys: "repos", "contributions", "avatar", "notifications", "repo_detail_fast", "repo_detail_stats")
    pub loading: HashSet<String>,

    // Error
    pub error: Option<String>,
    pub error_at: Option<Instant>,

    // UI overlays
    pub show_help: bool,
    pub search_mode: bool,
    pub search_query: String,
    pub search_input: String,

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
            home_focus: HomeFocus::Repos,
            repo_filter: RepoFilter::All,
            filter_index: 0,
            exclude_orgs: Vec::new(),
            exclude_repos: Vec::new(),
            detail_focus: DetailFocus::TabBar,
            repo_prs: Vec::new(),
            repo_issues: Vec::new(),
            repo_ci: Vec::new(),
            detail_selected: 0,
            repo_commits: Vec::new(),
            repo_commit_activity: Vec::new(),
            repo_readme: None,
            repo_languages: Vec::new(),
            repo_contributors: Vec::new(),
            repo_code_frequency: Vec::new(),
            detail_cache_saved_at: None,
            notifications: Vec::new(),
            notif_selected: 0,
            show_notifications: false,
            loading: HashSet::new(),
            error: None,
            error_at: None,
            show_help: false,
            search_mode: false,
            search_query: String::new(),
            search_input: String::new(),
            pending_open_url: None,
            term_width: 120,
            term_height: 40,
        }
    }

    /// Number of card columns based on terminal width.
    pub fn num_card_cols(&self) -> usize {
        if self.term_width >= 120 {
            3
        } else if self.term_width >= 80 {
            2
        } else {
            1
        }
    }

    /// Build the list of filter options from loaded repos.
    pub fn filter_options(&self) -> Vec<RepoFilter> {
        let mut opts = vec![RepoFilter::All, RepoFilter::Public, RepoFilter::Private];
        let username = self
            .user_info
            .as_ref()
            .map(|u| u.login.as_str())
            .unwrap_or("");
        let mut orgs: Vec<&str> = self
            .repos
            .iter()
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
        let active_query = if self.search_mode {
            self.search_input.as_str()
        } else {
            self.search_query.as_str()
        };
        let current_login = self.user_info.as_ref().map(|u| u.login.as_str());
        let filter_iter = self.repos.iter().filter(|r| match &self.repo_filter {
            RepoFilter::All => true,
            RepoFilter::Public => {
                !r.is_private && current_login.map(|login| r.owner == login).unwrap_or(true)
            }
            RepoFilter::Private => {
                r.is_private && current_login.map(|login| r.owner == login).unwrap_or(true)
            }
            RepoFilter::Org(name) => r.owner == *name,
        });

        if active_query.is_empty() {
            return filter_iter.collect();
        }
        let q = active_query.to_lowercase();
        filter_iter
            .filter(|r| {
                r.full_name.to_lowercase().contains(&q)
                    || r.description
                        .as_ref()
                        .is_some_and(|d| d.to_lowercase().contains(&q))
                    || r.language
                        .as_ref()
                        .is_some_and(|l| l.to_lowercase().contains(&q))
            })
            .collect()
    }

    /// Get the currently selected repo (if any), accounting for search filter.
    pub fn selected_repo(&self) -> Option<&RepoInfo> {
        self.filtered_repos().get(self.card_selected).copied()
    }

    /// Get repo detail items filtered by search query.
    pub fn filtered_detail_items(&self) -> Vec<DetailItem<'_>> {
        match &self.screen {
            Screen::RepoDetail {
                section: RepoSection::PRs,
                ..
            } => {
                let mut open: Vec<_> = self.repo_prs.iter().filter(|p| p.state == "open" && !p.draft).collect();
                let mut draft: Vec<_> = self.repo_prs.iter().filter(|p| p.draft && p.state == "open").collect();
                let mut merged: Vec<_> = self.repo_prs.iter().filter(|p| p.merged).collect();
                let mut closed: Vec<_> = self.repo_prs.iter().filter(|p| p.state == "closed" && !p.merged).collect();

                let sort_by_updated = |a: &&PrInfo, b: &&PrInfo| b.updated_at.cmp(&a.updated_at);
                open.sort_by(sort_by_updated);
                draft.sort_by(sort_by_updated);
                merged.sort_by(sort_by_updated);
                closed.sort_by(sort_by_updated);

                let mut items = Vec::new();
                for (label, group) in [("Open", open), ("Draft", draft), ("Merged", merged), ("Closed", closed)] {
                    if !group.is_empty() {
                        items.push(DetailItem::SectionHeader(format!("{} ({})", label, group.len())));
                        items.extend(group.into_iter().map(DetailItem::Pr));
                    }
                }
                items
            }
            Screen::RepoDetail {
                section: RepoSection::Issues,
                ..
            } => {
                let mut open: Vec<_> = self.repo_issues.iter().filter(|i| i.state == "open").collect();
                let mut closed: Vec<_> = self.repo_issues.iter().filter(|i| i.state == "closed").collect();

                let sort_by_updated = |a: &&IssueInfo, b: &&IssueInfo| b.updated_at.cmp(&a.updated_at);
                open.sort_by(sort_by_updated);
                closed.sort_by(sort_by_updated);

                let mut items = Vec::new();
                for (label, group) in [("Open", open), ("Closed", closed)] {
                    if !group.is_empty() {
                        items.push(DetailItem::SectionHeader(format!("{} ({})", label, group.len())));
                        items.extend(group.into_iter().map(DetailItem::Issue));
                    }
                }
                items
            }
            Screen::RepoDetail {
                section: RepoSection::CI,
                ..
            } => self.repo_ci.iter().map(|r| DetailItem::Ci(r)).collect(),
            Screen::RepoDetail {
                section: RepoSection::Commits,
                ..
            } => self
                .repo_commits
                .iter()
                .map(|c| DetailItem::Commit(c))
                .collect(),
            Screen::RepoDetail {
                section: RepoSection::Info,
                ..
            } => Vec::new(),
            _ => Vec::new(),
        }
    }

    /// Filtered notifications.
    pub fn filtered_notifications(&self) -> Vec<&Notification> {
        self.notifications.iter().collect()
    }

    /// Count of unread notifications.
    pub fn unread_count(&self) -> usize {
        self.notifications.iter().filter(|n| n.unread).count()
    }

    /// Breadcrumb path for header bar.
    pub fn breadcrumb(&self) -> String {
        match &self.screen {
            Screen::Home => "Home".to_string(),
            Screen::RepoDetail {
                repo_full_name,
                section,
            } => {
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
    Commit(&'a CommitInfo),
    SectionHeader(String),
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
    ToggleViewMode,
    ToggleListMode,

    // Data loaded
    ReposLoaded(Vec<RepoInfo>),
    RepoOpenCountsLoaded(Vec<RepoOpenCounts>),
    ContributionsLoaded(ContributionData),
    AvatarLoaded(Vec<u8>),
    UserInfoLoaded(UserInfo),
    NotificationsLoaded(Vec<Notification>),
    RepoDetailFromCacheLoaded {
        repo: String,
        cached_at: DateTime<Utc>,
        prs: Vec<PrInfo>,
        issues: Vec<IssueInfo>,
        ci: Vec<WorkflowRun>,
        commits: Vec<CommitInfo>,
        commit_activity: Vec<WeeklyCommitActivity>,
        readme: Option<String>,
        languages: Vec<(String, u64)>,
        contributors: Vec<ContributorInfo>,
        code_frequency: Vec<(i64, i64, i64)>,
    },
    RepoDetailFastLoaded {
        repo: String,
        prs: Vec<PrInfo>,
        issues: Vec<IssueInfo>,
        ci: Vec<WorkflowRun>,
        commits: Vec<CommitInfo>,
        readme: Option<String>,
        languages: Vec<(String, u64)>,
    },
    RepoDetailStatsLoaded {
        repo: String,
        commit_activity: Vec<WeeklyCommitActivity>,
        contributors: Vec<ContributorInfo>,
        code_frequency: Vec<(i64, i64, i64)>,
    },

    // Actions
    ToggleNotifications,
    MarkNotifRead(String),
    MarkAllNotifsRead,
    ForceRefresh,

    // UI
    Tick,
    Error(String),
    ToggleHelp,
    EnterSearch,
    ConfirmSearch,
    CancelSearch,
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
        Message::Resize(w, h) => {
            state.term_width = w;
            state.term_height = h;
        }

        // ── Data loaded ──
        Message::ReposLoaded(repos) => {
            let cached_counts: HashMap<String, (Option<u32>, Option<u32>)> = state
                .repos
                .iter()
                .map(|r| {
                    (
                        r.full_name.clone(),
                        (r.open_issues_only_count, r.open_prs_count),
                    )
                })
                .collect();

            state.repos = repos
                .into_iter()
                .filter(|r| {
                    !state
                        .exclude_orgs
                        .iter()
                        .any(|o| o.eq_ignore_ascii_case(&r.owner))
                        && !state
                            .exclude_repos
                            .iter()
                            .any(|e| e.eq_ignore_ascii_case(&r.full_name))
                })
                .map(|mut r| {
                    if let Some((issues, prs)) = cached_counts.get(&r.full_name) {
                        if r.open_issues_only_count.is_none() {
                            r.open_issues_only_count = *issues;
                        }
                        if r.open_prs_count.is_none() {
                            r.open_prs_count = *prs;
                        }
                    }
                    r
                })
                .collect();
            state.loading.remove("repos");
        }
        Message::RepoOpenCountsLoaded(counts) => {
            for c in counts {
                if let Some(repo) = state.repos.iter_mut().find(|r| r.full_name == c.full_name) {
                    repo.open_issues_only_count = Some(c.open_issues_count);
                    repo.open_prs_count = Some(c.open_prs_count);
                }
            }
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
        Message::RepoDetailFromCacheLoaded {
            repo,
            cached_at,
            prs,
            issues,
            ci,
            commits,
            commit_activity,
            readme,
            languages,
            contributors,
            code_frequency,
        } => {
            if let Screen::RepoDetail { repo_full_name, .. } = &state.screen
                && *repo_full_name == repo
            {
                state.repo_prs = prs;
                state.repo_issues = issues;
                state.repo_ci = ci;
                state.repo_commits = commits;
                state.repo_commit_activity = commit_activity;
                state.repo_readme = readme;
                state.repo_languages = languages;
                state.repo_contributors = contributors;
                state.repo_code_frequency = code_frequency;
                state.detail_cache_saved_at = Some(cached_at);
            }
        }
        Message::RepoDetailFastLoaded {
            repo,
            prs,
            issues,
            ci,
            commits,
            readme,
            languages,
        } => {
            if let Screen::RepoDetail { repo_full_name, .. } = &state.screen
                && *repo_full_name == repo
            {
                state.repo_prs = prs;
                state.repo_issues = issues;
                state.repo_ci = ci;
                state.repo_commits = commits;
                state.repo_readme = readme;
                state.repo_languages = languages;
                state.detail_cache_saved_at = None;
                state.loading.remove("repo_detail_fast");
            }
        }
        Message::RepoDetailStatsLoaded {
            repo,
            commit_activity,
            contributors,
            code_frequency,
        } => {
            if let Screen::RepoDetail { repo_full_name, .. } = &state.screen
                && *repo_full_name == repo
            {
                state.repo_commit_activity = commit_activity;
                state.repo_contributors = contributors;
                state.repo_code_frequency = code_frequency;
                state.detail_cache_saved_at = None;
                state.loading.remove("repo_detail_stats");
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
                        if state.detail_focus == DetailFocus::TabBar {
                            // Already at top
                        } else if state.detail_selected == 0 {
                            state.detail_focus = DetailFocus::TabBar;
                        } else {
                            let prev = state.detail_selected - 1;
                            let prev_is_header = matches!(
                                state.filtered_detail_items().get(prev),
                                Some(DetailItem::SectionHeader(_))
                            );
                            if prev_is_header && prev == 0 {
                                state.detail_focus = DetailFocus::TabBar;
                            } else if prev_is_header {
                                state.detail_selected = prev - 1;
                            } else {
                                state.detail_selected = prev;
                            }
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
                    Screen::RepoDetail { section, .. } => {
                        if state.detail_focus == DetailFocus::TabBar {
                            state.detail_focus = DetailFocus::Content;
                            // Skip leading section header
                            let cur_is_header = matches!(
                                state.filtered_detail_items().get(state.detail_selected),
                                Some(DetailItem::SectionHeader(_))
                            );
                            if cur_is_header {
                                state.detail_selected += 1;
                            }
                        } else if *section == RepoSection::Info {
                            state.detail_selected += 1;
                        } else {
                            let items = state.filtered_detail_items();
                            let len = items.len();
                            let next = state.detail_selected + 1;
                            let next_is_header = matches!(items.get(next), Some(DetailItem::SectionHeader(_)));
                            let skip_target = if next_is_header { next + 1 } else { next };
                            drop(items);
                            if skip_target < len {
                                state.detail_selected = skip_target;
                            }
                        }
                    }
                }
            }
        }

        Message::Left => match &state.screen {
            Screen::Home => {
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
            Screen::RepoDetail { .. } => {
                if state.detail_focus == DetailFocus::TabBar {
                    if let Screen::RepoDetail { section, .. } = &mut state.screen {
                        *section = section.prev();
                        state.detail_selected = 0;
                    }
                }
            }
        },

        Message::Right => match &state.screen {
            Screen::Home => {
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
            Screen::RepoDetail { .. } => {
                if state.detail_focus == DetailFocus::TabBar {
                    if let Screen::RepoDetail { section, .. } = &mut state.screen {
                        *section = section.next();
                        state.detail_selected = 0;
                    }
                }
            }
        },

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
                                state.detail_focus = DetailFocus::TabBar;
                                state.repo_prs.clear();
                                state.repo_issues.clear();
                                state.repo_ci.clear();
                                state.repo_commits.clear();
                                state.repo_commit_activity.clear();
                                state.repo_readme = None;
                                state.repo_languages.clear();
                                state.repo_contributors.clear();
                                state.repo_code_frequency.clear();
                                state.detail_cache_saved_at = None;
                                state.loading.insert("repo_detail_fast".to_string());
                                state.loading.insert("repo_detail_stats".to_string());
                                state.search_mode = false;
                            }
                        }
                    }
                    Screen::RepoDetail { .. } => {
                        if state.detail_focus == DetailFocus::TabBar {
                            state.detail_focus = DetailFocus::Content;
                            // Skip leading section header
                            let cur_is_header = matches!(
                                state.filtered_detail_items().get(state.detail_selected),
                                Some(DetailItem::SectionHeader(_))
                            );
                            if cur_is_header {
                                state.detail_selected += 1;
                            }
                            return;
                        }
                        // Open item in browser
                        let items = state.filtered_detail_items();
                        if let Some(item) = items.get(state.detail_selected) {
                            let url = match item {
                                DetailItem::Pr(pr) => &pr.html_url,
                                DetailItem::Issue(i) => &i.html_url,
                                DetailItem::Ci(r) => &r.html_url,
                                DetailItem::Commit(c) => &c.html_url,
                                DetailItem::SectionHeader(_) => return,
                            };
                            state.pending_open_url = Some(url.clone());
                        }
                    }
                }
            }
        }

        Message::Back => {
            if state.show_help {
                state.show_help = false;
            } else if state.show_notifications {
                state.show_notifications = false;
            } else if state.search_mode {
                state.search_mode = false;
                state.search_input = state.search_query.clone();
            } else {
                match &state.screen {
                    Screen::RepoDetail { .. } => {
                        state.screen = Screen::Home;
                        state.detail_selected = 0;
                        state.detail_cache_saved_at = None;
                    }
                    Screen::Home => state.should_quit = true,
                }
            }
        }

        Message::GoHome => {
            state.screen = Screen::Home;
            state.show_notifications = false;
            state.search_mode = false;
            state.search_input = state.search_query.clone();
            state.detail_cache_saved_at = None;
        }

        Message::ToggleViewMode => {
            match &mut state.screen {
                Screen::Home => {
                    let opts = state.filter_options();
                    if opts.is_empty() {
                        return;
                    }

                    // Keep filter_index aligned even if options changed after data refresh.
                    let current = opts
                        .iter()
                        .position(|f| *f == state.repo_filter)
                        .unwrap_or(0);
                    let next = (current + 1) % opts.len();

                    state.filter_index = next;
                    state.repo_filter = opts[next].clone();
                    state.home_focus = HomeFocus::FilterBar;
                    state.card_selected = 0;
                }
                Screen::RepoDetail { section, .. } => {
                    *section = section.next();
                    state.detail_selected = 0;
                    state.detail_focus = DetailFocus::TabBar;
                }
            }
        }
        Message::ToggleListMode => {
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
        Message::EnterSearch => {
            if state.screen == Screen::Home {
                state.search_mode = true;
                state.search_input = state.search_query.clone();
            }
        }
        Message::ConfirmSearch => {
            state.search_query = state.search_input.clone();
            state.card_selected = 0;
            state.detail_selected = 0;
            state.notif_selected = 0;
            state.search_mode = false;
        }
        Message::CancelSearch => {
            state.search_mode = false;
            state.search_input = state.search_query.clone();
        }
        Message::SearchInput(c) => {
            state.search_input.push(c);
            state.card_selected = 0;
        }
        Message::SearchBackspace => {
            state.search_input.pop();
            state.card_selected = 0;
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
    fn test_error() {
        let mut state = AppState::new();
        update(&mut state, Message::Error("oops".into()));
        assert_eq!(state.error.as_deref(), Some("oops"));
        assert!(state.error_at.is_some());
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
            make_repo("a"),
            make_repo("b"),
            make_repo("c"),
            make_repo("d"),
            make_repo("e"),
            make_repo("f"),
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
        assert_eq!(
            state.screen,
            Screen::RepoDetail {
                repo_full_name: "user/myrepo".to_string(),
                section: RepoSection::PRs,
            }
        );
        assert!(state.loading.contains("repo_detail_fast"));
        assert!(state.loading.contains("repo_detail_stats"));
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
    fn test_left_right_navigates_sections() {
        let mut state = AppState::new();
        state.screen = Screen::RepoDetail {
            repo_full_name: "user/repo".to_string(),
            section: RepoSection::PRs,
        };
        state.detail_focus = DetailFocus::TabBar;

        update(&mut state, Message::Right);
        assert!(matches!(
            state.screen,
            Screen::RepoDetail {
                section: RepoSection::Issues,
                ..
            }
        ));
        update(&mut state, Message::Right);
        assert!(matches!(
            state.screen,
            Screen::RepoDetail {
                section: RepoSection::CI,
                ..
            }
        ));
        update(&mut state, Message::Right);
        assert!(matches!(
            state.screen,
            Screen::RepoDetail {
                section: RepoSection::Commits,
                ..
            }
        ));
        update(&mut state, Message::Right);
        assert!(matches!(
            state.screen,
            Screen::RepoDetail {
                section: RepoSection::Info,
                ..
            }
        ));
        update(&mut state, Message::Right);
        assert!(matches!(
            state.screen,
            Screen::RepoDetail {
                section: RepoSection::PRs,
                ..
            }
        ));

        // Left goes backward
        update(&mut state, Message::Left);
        assert!(matches!(
            state.screen,
            Screen::RepoDetail {
                section: RepoSection::Info,
                ..
            }
        ));
        update(&mut state, Message::Left);
        assert!(matches!(
            state.screen,
            Screen::RepoDetail {
                section: RepoSection::Commits,
                ..
            }
        ));
    }

    #[test]
    fn test_down_from_tabbar_enters_content() {
        let mut state = AppState::new();
        state.screen = Screen::RepoDetail {
            repo_full_name: "user/repo".to_string(),
            section: RepoSection::PRs,
        };
        state.detail_focus = DetailFocus::TabBar;

        update(&mut state, Message::Down);
        assert_eq!(state.detail_focus, DetailFocus::Content);
    }

    #[test]
    fn test_up_from_content_top_enters_tabbar() {
        let mut state = AppState::new();
        state.screen = Screen::RepoDetail {
            repo_full_name: "user/repo".to_string(),
            section: RepoSection::PRs,
        };
        state.detail_focus = DetailFocus::Content;
        state.detail_selected = 0;

        update(&mut state, Message::Up);
        assert_eq!(state.detail_focus, DetailFocus::TabBar);
    }

    #[test]
    fn test_toggle_view_mode() {
        let mut state = AppState::new();
        state.repos = vec![
            make_repo("alice/public1"),
            make_repo_private("alice/secret"),
            make_repo("acme-corp/tool"),
        ];
        state.user_info = Some(crate::github::UserInfo {
            login: "alice".to_string(),
            avatar_url: String::new(),
            public_repos: 2,
            followers: 0,
            name: None,
            bio: None,
            location: None,
            company: None,
        });

        assert_eq!(state.repo_filter, RepoFilter::All);
        update(&mut state, Message::ToggleViewMode);
        assert_eq!(state.repo_filter, RepoFilter::Public);
        update(&mut state, Message::ToggleViewMode);
        assert_eq!(state.repo_filter, RepoFilter::Private);
    }

    #[test]
    fn test_toggle_view_mode_cycles_repo_detail_sections() {
        let mut state = AppState::new();
        state.screen = Screen::RepoDetail {
            repo_full_name: "user/repo".to_string(),
            section: RepoSection::PRs,
        };
        state.detail_focus = DetailFocus::Content;
        state.detail_selected = 2;

        update(&mut state, Message::ToggleViewMode);
        assert!(matches!(
            state.screen,
            Screen::RepoDetail {
                section: RepoSection::Issues,
                ..
            }
        ));
        assert_eq!(state.detail_selected, 0);
        assert_eq!(state.detail_focus, DetailFocus::TabBar);
    }

    #[test]
    fn test_toggle_list_mode() {
        let mut state = AppState::new();
        assert_eq!(state.view_mode, ViewMode::Cards);
        update(&mut state, Message::ToggleListMode);
        assert_eq!(state.view_mode, ViewMode::List);
        update(&mut state, Message::ToggleListMode);
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
        state.search_mode = true;
        update(&mut state, Message::SearchInput('a'));
        assert_eq!(state.card_selected, 0);
        assert_eq!(state.search_input, "a");
    }

    #[test]
    fn test_filtered_repos_use_live_search_input_while_searching() {
        let mut state = AppState::new();
        state.repos = vec![make_repo("alice/alpha"), make_repo("alice/beta")];
        state.search_query = "alpha".to_string();
        state.search_mode = true;
        state.search_input = "beta".to_string();

        let filtered = state.filtered_repos();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].full_name, "alice/beta");
    }

    #[test]
    fn test_enter_search_prefills_input() {
        let mut state = AppState::new();
        state.search_query = "ghd".to_string();

        update(&mut state, Message::EnterSearch);
        assert!(state.search_mode);
        assert_eq!(state.search_input, "ghd");
    }

    #[test]
    fn test_confirm_search_applies_query_and_resets_selection() {
        let mut state = AppState::new();
        state.search_mode = true;
        state.search_input = "core".to_string();
        state.card_selected = 4;
        state.detail_selected = 3;
        state.notif_selected = 2;

        update(&mut state, Message::ConfirmSearch);

        assert!(!state.search_mode);
        assert_eq!(state.search_query, "core");
        assert_eq!(state.card_selected, 0);
        assert_eq!(state.detail_selected, 0);
        assert_eq!(state.notif_selected, 0);
    }

    #[test]
    fn test_cancel_search_keeps_applied_query() {
        let mut state = AppState::new();
        state.search_query = "old".to_string();
        state.search_input = "new".to_string();
        state.search_mode = true;

        update(&mut state, Message::CancelSearch);

        assert!(!state.search_mode);
        assert_eq!(state.search_query, "old");
        assert_eq!(state.search_input, "old");
    }

    #[test]
    fn test_back_in_search_mode_keeps_applied_query() {
        let mut state = AppState::new();
        state.search_query = "repos".to_string();
        state.search_input = "repos-x".to_string();
        state.search_mode = true;

        update(&mut state, Message::Back);

        assert!(!state.search_mode);
        assert_eq!(state.search_query, "repos");
        assert_eq!(state.search_input, "repos");
    }

    #[test]
    fn test_enter_search_only_on_home() {
        let mut state = AppState::new();
        state.screen = Screen::RepoDetail {
            repo_full_name: "user/repo".to_string(),
            section: RepoSection::PRs,
        };

        update(&mut state, Message::EnterSearch);
        assert!(!state.search_mode);
    }

    #[test]
    fn test_filtered_notifications_ignore_search_query() {
        let mut state = AppState::new();
        state.notifications = vec![
            make_notification("1", true),
            make_notification_with_title("2", true, "Different title"),
        ];
        state.search_query = "does-not-match".to_string();

        assert_eq!(state.filtered_notifications().len(), 2);
    }

    #[test]
    fn test_filtered_detail_items_ignore_search_query() {
        let mut state = AppState::new();
        state.screen = Screen::RepoDetail {
            repo_full_name: "user/repo".to_string(),
            section: RepoSection::PRs,
        };
        state.repo_prs = vec![make_pr("Fix search flow", "alice")];
        state.search_query = "does-not-match".to_string();

        // 1 section header + 1 PR item
        assert_eq!(state.filtered_detail_items().len(), 2);
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
            name: None,
            bio: None,
            location: None,
            company: None,
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
            make_repo_private("acme-corp/secret-tool"),
        ];
        state.user_info = Some(crate::github::UserInfo {
            login: "alice".to_string(),
            avatar_url: String::new(),
            public_repos: 2,
            followers: 0,
            name: None,
            bio: None,
            location: None,
            company: None,
        });

        state.repo_filter = RepoFilter::All;
        assert_eq!(state.filtered_repos().len(), 4);

        state.repo_filter = RepoFilter::Public;
        assert_eq!(state.filtered_repos().len(), 1);

        state.repo_filter = RepoFilter::Private;
        assert_eq!(state.filtered_repos().len(), 1);

        state.repo_filter = RepoFilter::Org("acme-corp".to_string());
        assert_eq!(state.filtered_repos().len(), 2);
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
            name: None,
            bio: None,
            location: None,
            company: None,
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
            open_issues_only_count: None,
            open_prs_count: None,
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

    fn make_notification_with_title(id: &str, unread: bool, title: &str) -> Notification {
        Notification {
            id: id.to_string(),
            reason: "subscribed".to_string(),
            subject_title: title.to_string(),
            subject_type: "PullRequest".to_string(),
            repo_full_name: "user/repo".to_string(),
            updated_at: None,
            unread,
            url: Some("https://github.com/user/repo/pull/1".to_string()),
        }
    }

    fn make_pr(title: &str, user: &str) -> PrInfo {
        PrInfo {
            number: 1,
            title: title.to_string(),
            repo_full_name: "user/repo".to_string(),
            state: "open".to_string(),
            html_url: "https://github.com/user/repo/pull/1".to_string(),
            created_at: None,
            updated_at: None,
            draft: false,
            user: user.to_string(),
            head_ref: "feature".to_string(),
            base_ref: "main".to_string(),
            merged: false,
            additions: 0,
            deletions: 0,
        }
    }
}
