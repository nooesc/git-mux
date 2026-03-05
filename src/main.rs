mod app;
mod cache;
mod config;
mod event;
mod github;
mod ui;
mod workspace;

use anyhow::Result;
use app::{AppState, Message, Screen, update};
use crossterm::event::KeyCode;
use event::{AppEvent, EventHandler};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::{DefaultTerminal, Frame};
use std::time::Duration;

fn main() -> Result<()> {
    let mut terminal = ratatui::init();
    let result = run(&mut terminal);
    ratatui::restore();
    result
}

fn run(terminal: &mut DefaultTerminal) -> Result<()> {
    let config = config::Config::load()?;

    // Clean up old workspaces on startup
    {
        let ws_ops = workspace::WorkspaceOps::new(config.workspaces.dir.clone());
        if let Err(e) = ws_ops.cleanup_old_workspaces(config.workspaces.cleanup_after_days) {
            eprintln!("Warning: workspace cleanup failed: {}", e);
        }
    }

    let mut state = AppState::new();
    state.exclude_orgs = config.orgs.exclude.clone();
    state.exclude_repos = config.repos.exclude.clone();

    // Get initial terminal size
    let size = terminal.size()?;
    state.term_width = size.width;
    state.term_height = size.height;

    let events = EventHandler::new(Duration::from_millis(200));

    let rt = tokio::runtime::Runtime::new()?;
    let (bg_tx, bg_rx) = std::sync::mpsc::channel::<Message>();

    // Warm start: apply disk cache immediately, then refresh in background.
    let cached_startup = crate::cache::load_startup_cache();
    let mut has_cached_repos = false;
    let mut has_cached_contributions = false;
    let mut has_cached_avatar = false;
    let mut has_cached_notifications = false;

    if let Some(cache) = cached_startup.as_ref() {
        if let Some(info) = cache.user_info.clone() {
            update(&mut state, Message::UserInfoLoaded(info));
        }
        if let Some(repos) = cache.repos.clone() {
            has_cached_repos = true;
            update(&mut state, Message::ReposLoaded(repos));
        }
        if let Some(contrib) = cache.contributions.clone() {
            has_cached_contributions = true;
            update(&mut state, Message::ContributionsLoaded(contrib));
        }
        if let Some(avatar_png) = cache.avatar_png.clone() {
            has_cached_avatar = true;
            update(&mut state, Message::AvatarLoaded(avatar_png));
        }
        if let Some(notifications) = cache.notifications.clone() {
            has_cached_notifications = true;
            update(&mut state, Message::NotificationsLoaded(notifications));
        }
    }

    // ── Startup fetch: repos, contributions, notifications, avatar ──
    {
        let tx = bg_tx.clone();
        let cache_for_merge = cached_startup.clone();
        rt.spawn(async move {
            let client = match crate::github::GitHubClient::new().await {
                Ok(c) => c,
                Err(e) => {
                    let _ = tx.send(Message::Error(format!("Auth failed: {}", e)));
                    return;
                }
            };

            // Send user info immediately
            let _ = tx.send(Message::UserInfoLoaded(client.user_info.clone()));

            // Fetch avatar
            let avatar_url = client.user_info.avatar_url.clone();
            let tx_avatar = tx.clone();
            let avatar_handle = tokio::spawn(async move {
                if let Ok(img) = crate::github::avatar::download_avatar(&avatar_url).await {
                    let mut buf = std::io::Cursor::new(Vec::new());
                    if img.write_to(&mut buf, image::ImageFormat::Png).is_ok() {
                        let bytes = buf.into_inner();
                        let _ = tx_avatar.send(Message::AvatarLoaded(bytes.clone()));
                        return Some(bytes);
                    }
                }
                None::<Vec<u8>>
            });

            // Fetch repos, contributions, notifications in parallel
            let (tx_repos, tx_contrib, tx_notif) = (tx.clone(), tx.clone(), tx.clone());
            let (c1, c2, c3) = (client.clone(), client.clone(), client.clone());

            let repos_handle = tokio::spawn(async move {
                match c1.fetch_all_repos().await {
                    Ok(mut repos) => {
                        let _ = tx_repos.send(Message::ReposLoaded(repos.clone()));

                        match c1.fetch_repo_open_counts(&repos).await {
                            Ok(counts) => {
                                for c in &counts {
                                    if let Some(repo) =
                                        repos.iter_mut().find(|r| r.full_name == c.full_name)
                                    {
                                        repo.open_issues_only_count = Some(c.open_issues_count);
                                        repo.open_prs_count = Some(c.open_prs_count);
                                    }
                                }
                                let _ = tx_repos.send(Message::RepoOpenCountsLoaded(counts));
                            }
                            Err(_) => {}
                        }

                        Some(repos)
                    }
                    Err(e) => {
                        let _ = tx_repos.send(Message::Error(format!("Repos: {}", e)));
                        None
                    }
                }
            });

            let contrib_handle = tokio::spawn(async move {
                match c2.fetch_contributions().await {
                    Ok(data) => {
                        let _ = tx_contrib.send(Message::ContributionsLoaded(data.clone()));
                        Some(data)
                    }
                    Err(e) => {
                        let _ = tx_contrib.send(Message::Error(format!("Contributions: {}", e)));
                        None
                    }
                }
            });

            let notif_handle = tokio::spawn(async move {
                match c3.fetch_notifications().await {
                    Ok(notifs) => {
                        let _ = tx_notif.send(Message::NotificationsLoaded(notifs.clone()));
                        Some(notifs)
                    }
                    Err(e) => {
                        let _ = tx_notif.send(Message::Error(format!("Notifications: {}", e)));
                        None
                    }
                }
            });

            let (avatar, repos, contributions, notifications) =
                tokio::join!(avatar_handle, repos_handle, contrib_handle, notif_handle);

            let mut cache = crate::cache::StartupCache::fresh_from_previous(cache_for_merge);
            cache.user_info = Some(client.user_info.clone());
            if let Ok(Some(bytes)) = avatar {
                cache.avatar_png = Some(bytes);
            }
            if let Ok(Some(items)) = repos {
                cache.repos = Some(items);
            }
            if let Ok(Some(data)) = contributions {
                cache.contributions = Some(data);
            }
            if let Ok(Some(items)) = notifications {
                cache.notifications = Some(items);
            }

            let _ = crate::cache::save_startup_cache(&cache);
        });

        if !has_cached_repos {
            state.loading.insert("repos".to_string());
        }
        if !has_cached_contributions {
            state.loading.insert("contributions".to_string());
        }
        if !has_cached_avatar {
            state.loading.insert("avatar".to_string());
        }
        if !has_cached_notifications {
            state.loading.insert("notifications".to_string());
        }
    }

    loop {
        // Drain background messages
        while let Ok(msg) = bg_rx.try_recv() {
            update(&mut state, msg);
        }

        terminal.draw(|frame| render(frame, &state))?;

        match events.next()? {
            AppEvent::Key(key) => {
                let msg = if state.search_mode {
                    match key.code {
                        KeyCode::Esc => Some(Message::CancelSearch),
                        KeyCode::Enter => Some(Message::ConfirmSearch),
                        KeyCode::Backspace => Some(Message::SearchBackspace),
                        KeyCode::Char(c) => Some(Message::SearchInput(c)),
                        _ => None,
                    }
                } else if state.show_help {
                    match key.code {
                        KeyCode::Esc => Some(Message::Quit),
                        KeyCode::Char('?') => Some(Message::ToggleHelp),
                        KeyCode::Char('q') => Some(Message::Back),
                        _ => None,
                    }
                } else if state.show_notifications {
                    match key.code {
                        KeyCode::Esc => Some(Message::Quit),
                        KeyCode::Char('n') => Some(Message::ToggleNotifications),
                        KeyCode::Char('q') => Some(Message::Back),
                        KeyCode::Char('j') | KeyCode::Down => Some(Message::Down),
                        KeyCode::Char('k') | KeyCode::Up => Some(Message::Up),
                        KeyCode::Enter => Some(Message::Select),
                        KeyCode::Char('m') => {
                            let filtered = state.filtered_notifications();
                            if let Some(notif) = filtered.get(state.notif_selected) {
                                let thread_id = notif.id.clone();
                                let tid = thread_id.clone();
                                rt.spawn(async move {
                                    if let Ok(client) = crate::github::GitHubClient::from_token() {
                                        let _ = client.mark_notification_read(&tid).await;
                                    }
                                });
                                Some(Message::MarkNotifRead(thread_id))
                            } else {
                                None
                            }
                        }
                        KeyCode::Char('a') => {
                            let ids: Vec<String> = state
                                .notifications
                                .iter()
                                .filter(|n| n.unread)
                                .map(|n| n.id.clone())
                                .collect();
                            rt.spawn(async move {
                                if let Ok(client) = crate::github::GitHubClient::from_token() {
                                    for id in ids {
                                        let _ = client.mark_notification_read(&id).await;
                                    }
                                }
                            });
                            Some(Message::MarkAllNotifsRead)
                        }
                        _ => None,
                    }
                } else if state.branch_input_mode {
                    match key.code {
                        KeyCode::Esc => Some(Message::BranchInputCancel),
                        KeyCode::Enter => Some(Message::BranchInputSubmit),
                        KeyCode::Backspace => Some(Message::BranchInputBackspace),
                        KeyCode::Char(c) => Some(Message::BranchInputChar(c)),
                        _ => None,
                    }
                } else if state.action_menu.is_some() {
                    match key.code {
                        KeyCode::Esc | KeyCode::Char('q') => Some(Message::ActionMenuDismiss),
                        KeyCode::Enter => Some(Message::ActionMenuSelect),
                        KeyCode::Char('j') | KeyCode::Down => Some(Message::ActionMenuDown),
                        KeyCode::Char('k') | KeyCode::Up => Some(Message::ActionMenuUp),
                        _ => None,
                    }
                } else {
                    match key.code {
                        KeyCode::Char('q') => Some(Message::Back),
                        KeyCode::Char('?') => Some(Message::ToggleHelp),
                        KeyCode::Char('/') if state.screen == Screen::Home => {
                            Some(Message::EnterSearch)
                        }
                        KeyCode::Char('n') => Some(Message::ToggleNotifications),

                        KeyCode::Char('j') | KeyCode::Down => Some(Message::Down),
                        KeyCode::Char('k') | KeyCode::Up => Some(Message::Up),
                        KeyCode::Char('h') | KeyCode::Left => Some(Message::Left),
                        KeyCode::Char('l') | KeyCode::Right => Some(Message::Right),

                        KeyCode::Enter => {
                            if state.screen == Screen::Home
                                && state.home_focus == app::HomeFocus::Repos
                                && let Some(repo) = state.selected_repo()
                            {
                                let repo_name = repo.full_name.clone();
                                let tx = bg_tx.clone();
                                rt.spawn(async move {
                                    let parts: Vec<&str> = repo_name.splitn(2, '/').collect();
                                    if parts.len() == 2 {
                                        let (owner, name) = (parts[0], parts[1]);
                                        let repo_full_name = format!("{}/{}", owner, name);
                                        let cached_detail =
                                            crate::cache::load_repo_detail_cache(&repo_full_name);

                                        if let Some(cache) = cached_detail.as_ref()
                                            && crate::cache::repo_detail_cache_is_fresh(
                                                cache.saved_at,
                                            )
                                        {
                                            let _ = tx.send(Message::RepoDetailFromCacheLoaded {
                                                repo: repo_full_name.clone(),
                                                cached_at: cache.saved_at,
                                                prs: cache.prs.clone(),
                                                issues: cache.issues.clone(),
                                                ci: cache.ci.clone(),
                                                commits: cache.commits.clone(),
                                                commit_activity: cache.commit_activity.clone(),
                                                readme: cache.readme.clone(),
                                                languages: cache.languages.clone(),
                                                contributors: cache.contributors.clone(),
                                                code_frequency: cache.code_frequency.clone(),
                                            });
                                        }

                                        if let Ok(client) =
                                            crate::github::GitHubClient::from_token()
                                        {
                                            // Phase 1: fast sections.
                                            let (prs, issues, ci, commits, readme, languages) = tokio::join!(
                                                client.fetch_repo_prs(owner, name),
                                                client.fetch_repo_issues(owner, name),
                                                client.fetch_repo_ci(owner, name),
                                                client.fetch_repo_commits(owner, name),
                                                client.fetch_readme(owner, name),
                                                client.fetch_languages(owner, name),
                                            );

                                            let fast_prs = prs.unwrap_or_default();
                                            let fast_issues = issues.unwrap_or_default();
                                            let fast_ci = ci.unwrap_or_default();
                                            let fast_commits = commits.unwrap_or_default();
                                            let fast_readme = readme.ok();
                                            let fast_languages = languages.unwrap_or_default();

                                            let _ = tx.send(Message::RepoDetailFastLoaded {
                                                repo: repo_full_name.clone(),
                                                prs: fast_prs.clone(),
                                                issues: fast_issues.clone(),
                                                ci: fast_ci.clone(),
                                                commits: fast_commits.clone(),
                                                readme: fast_readme.clone(),
                                                languages: fast_languages.clone(),
                                            });

                                            let mut merged_cache = cached_detail.unwrap_or_else(|| {
                                                crate::cache::RepoDetailCache::new(
                                                    repo_full_name.clone(),
                                                )
                                            });
                                            merged_cache.saved_at = chrono::Utc::now();
                                            merged_cache.prs = fast_prs;
                                            merged_cache.issues = fast_issues;
                                            merged_cache.ci = fast_ci;
                                            merged_cache.commits = fast_commits;
                                            merged_cache.readme = fast_readme;
                                            merged_cache.languages = fast_languages;
                                            let _ = crate::cache::save_repo_detail_cache(&merged_cache);

                                            // Phase 2: slow stats endpoints.
                                            let (commit_activity, contributors, code_frequency) = tokio::join!(
                                                client.fetch_commit_activity(owner, name),
                                                client.fetch_contributors(owner, name),
                                                client.fetch_code_frequency(owner, name),
                                            );

                                            let stats_commit_activity =
                                                commit_activity.unwrap_or_default();
                                            let stats_contributors =
                                                contributors.unwrap_or_default();
                                            let stats_code_frequency =
                                                code_frequency.unwrap_or_default();

                                            let _ = tx.send(Message::RepoDetailStatsLoaded {
                                                repo: repo_full_name.clone(),
                                                commit_activity: stats_commit_activity.clone(),
                                                contributors: stats_contributors.clone(),
                                                code_frequency: stats_code_frequency.clone(),
                                            });

                                            merged_cache.saved_at = chrono::Utc::now();
                                            merged_cache.commit_activity = stats_commit_activity;
                                            merged_cache.contributors = stats_contributors;
                                            merged_cache.code_frequency = stats_code_frequency;
                                            let _ = crate::cache::save_repo_detail_cache(&merged_cache);
                                        } else {
                                            if let Some(cache) = cached_detail.as_ref() {
                                                let _ = tx.send(Message::RepoDetailFastLoaded {
                                                    repo: repo_full_name.clone(),
                                                    prs: cache.prs.clone(),
                                                    issues: cache.issues.clone(),
                                                    ci: cache.ci.clone(),
                                                    commits: cache.commits.clone(),
                                                    readme: cache.readme.clone(),
                                                    languages: cache.languages.clone(),
                                                });
                                                let _ = tx.send(Message::RepoDetailStatsLoaded {
                                                    repo: repo_full_name.clone(),
                                                    commit_activity: cache.commit_activity.clone(),
                                                    contributors: cache.contributors.clone(),
                                                    code_frequency: cache.code_frequency.clone(),
                                                });
                                            } else {
                                                let _ = tx.send(Message::RepoDetailFastLoaded {
                                                    repo: repo_full_name.clone(),
                                                    prs: Vec::new(),
                                                    issues: Vec::new(),
                                                    ci: Vec::new(),
                                                    commits: Vec::new(),
                                                    readme: None,
                                                    languages: Vec::new(),
                                                });
                                                let _ = tx.send(Message::RepoDetailStatsLoaded {
                                                    repo: repo_full_name.clone(),
                                                    commit_activity: Vec::new(),
                                                    contributors: Vec::new(),
                                                    code_frequency: Vec::new(),
                                                });
                                            }
                                            let _ = tx.send(Message::Error(
                                                "Failed to authenticate GitHub client".to_string(),
                                            ));
                                        }
                                    }
                                });
                            }
                            // On Issues/PRs tab in Content focus -> show action menu
                            if let Screen::RepoDetail { section: app::RepoSection::PRs | app::RepoSection::Issues, .. } = &state.screen {
                                if state.detail_focus == app::DetailFocus::Content {
                                    Some(Message::ShowActionMenu)
                                } else {
                                    Some(Message::Select)
                                }
                            } else {
                                Some(Message::Select)
                            }
                        }

                        KeyCode::Esc => Some(Message::Quit),

                        KeyCode::Char('g') => Some(Message::GoHome),

                        KeyCode::Tab => Some(Message::ToggleViewMode),
                        KeyCode::Char('V') => Some(Message::ToggleListMode),

                        KeyCode::Char('r') => match &state.screen {
                            Screen::RepoDetail {
                                section: crate::app::RepoSection::CI,
                                ..
                            } => {
                                let items = state.filtered_detail_items();
                                if let Some(crate::app::DetailItem::Ci(run)) =
                                    items.get(state.detail_selected)
                                {
                                    let parts: Vec<&str> =
                                        run.repo_full_name.splitn(2, '/').collect();
                                    if parts.len() == 2 {
                                        let owner = parts[0].to_string();
                                        let repo = parts[1].to_string();
                                        let run_id = run.id;
                                        let tx = bg_tx.clone();
                                        rt.spawn(async move {
                                            if let Ok(client) =
                                                crate::github::GitHubClient::from_token()
                                                && let Err(e) = client
                                                    .rerun_workflow(&owner, &repo, run_id)
                                                    .await
                                            {
                                                let _ = tx.send(Message::Error(format!(
                                                    "Re-run failed: {}",
                                                    e
                                                )));
                                            }
                                        });
                                    }
                                }
                                None
                            }
                            _ => {
                                if !state.loading.contains("repos") {
                                    state.loading.insert("repos".to_string());
                                    let tx = bg_tx.clone();
                                    rt.spawn(async move {
                                        if let Ok(client) =
                                            crate::github::GitHubClient::from_token()
                                            && let Ok(mut repos) = client.fetch_all_repos().await
                                        {
                                            let _ = tx.send(Message::ReposLoaded(repos.clone()));
                                            if let Ok(counts) =
                                                client.fetch_repo_open_counts(&repos).await
                                            {
                                                for c in &counts {
                                                    if let Some(repo) = repos
                                                        .iter_mut()
                                                        .find(|r| r.full_name == c.full_name)
                                                    {
                                                        repo.open_issues_only_count =
                                                            Some(c.open_issues_count);
                                                        repo.open_prs_count =
                                                            Some(c.open_prs_count);
                                                    }
                                                }
                                                let _ =
                                                    tx.send(Message::RepoOpenCountsLoaded(counts));
                                            }
                                        }
                                    });
                                }
                                Some(Message::ForceRefresh)
                            }
                        },

                        KeyCode::Char('w') => {
                            if state.screen == Screen::Home
                                && state.home_focus == app::HomeFocus::Repos
                            {
                                Some(Message::ShowActionMenu)
                            } else {
                                None
                            }
                        }

                        _ => None,
                    }
                };

                if let Some(msg) = msg {
                    update(&mut state, msg);

                    if let Some(url) = state.pending_open_url.take() {
                        let _ = open::that(&url);
                    }

                    if state.pending_continue_locally {
                        state.pending_continue_locally = false;
                        if let Some(repo) = state.selected_repo() {
                            let parts: Vec<&str> =
                                repo.full_name.splitn(2, '/').collect();
                            if parts.len() == 2 {
                                let owner = parts[0].to_string();
                                let repo_name = parts[1].to_string();
                                let source_dirs =
                                    config.workspaces.source_dirs.clone();
                                let tx = bg_tx.clone();
                                rt.spawn(async move {
                                    let result =
                                        tokio::task::spawn_blocking(move || {
                                            let source =
                                                workspace::find_source_repo(
                                                    &source_dirs,
                                                    &owner,
                                                    &repo_name,
                                                );
                                            match source {
                                                Some(path) => {
                                                    let dir_name = format!(
                                                        "{}/{}",
                                                        owner, repo_name
                                                    );
                                                    if workspace::is_inside_tmux()
                                                    {
                                                        workspace::open_tmux_window(
                                                            &path, &dir_name,
                                                        )?;
                                                    }
                                                    Ok::<String, anyhow::Error>(
                                                        path.to_string_lossy()
                                                            .to_string(),
                                                    )
                                                }
                                                None => Err(anyhow::anyhow!(
                                                    "No local repo found for {}/{}. Configure workspaces.source_dirs in config.toml",
                                                    owner,
                                                    repo_name
                                                )),
                                            }
                                        })
                                        .await
                                        .map_err(|e| {
                                            anyhow::anyhow!(
                                                "Task join error: {}",
                                                e
                                            )
                                        })?;
                                    match result {
                                        Ok(path) => {
                                            let _ = tx.send(
                                                Message::WorkspaceReady(path),
                                            );
                                        }
                                        Err(e) => {
                                            let _ = tx.send(
                                                Message::WorkspaceError(
                                                    e.to_string(),
                                                ),
                                            );
                                        }
                                    }
                                    Ok::<(), anyhow::Error>(())
                                });
                            }
                        }
                    }

                    if state.pending_start_work {
                        state.pending_start_work = false;
                        let workspace_config = config.workspaces.clone();

                        // Determine what to clone based on current screen/selection
                        let clone_info: Option<(String, String, String, String, Option<u64>)> =
                            match &state.screen {
                                Screen::RepoDetail {
                                    repo_full_name,
                                    section,
                                } => {
                                    let parts: Vec<&str> =
                                        repo_full_name.splitn(2, '/').collect();
                                    if parts.len() != 2 {
                                        None
                                    } else {
                                        let (owner, repo) =
                                            (parts[0].to_string(), parts[1].to_string());
                                        let items = state.filtered_detail_items();
                                        match section {
                                            app::RepoSection::Issues => {
                                                if let Some(app::DetailItem::Issue(issue)) =
                                                    items.get(state.detail_selected)
                                                {
                                                    let branch = workspace::issue_branch_name(
                                                        issue.number,
                                                        &issue.title,
                                                    );
                                                    let dir_slug = branch.clone();
                                                    Some((
                                                        owner, repo, branch, dir_slug, None,
                                                    ))
                                                } else {
                                                    None
                                                }
                                            }
                                            app::RepoSection::PRs => {
                                                if let Some(app::DetailItem::Pr(pr)) =
                                                    items.get(state.detail_selected)
                                                {
                                                    let dir_slug = workspace::pr_dir_slug(
                                                        pr.number, &pr.title,
                                                    );
                                                    let branch = pr.head_ref.clone();
                                                    Some((
                                                        owner,
                                                        repo,
                                                        branch,
                                                        dir_slug,
                                                        Some(pr.number),
                                                    ))
                                                } else {
                                                    None
                                                }
                                            }
                                            _ => None,
                                        }
                                    }
                                }
                                Screen::Home => {
                                    if let Some(branch) = state.branch_input.take() {
                                        if let Err(e) =
                                            workspace::validate_branch_name(&branch)
                                        {
                                            state.error = Some(e.to_string());
                                            state.error_at =
                                                Some(std::time::Instant::now());
                                            None
                                        } else if let Some(repo) = state.selected_repo() {
                                            let parts: Vec<&str> =
                                                repo.full_name.splitn(2, '/').collect();
                                            if parts.len() == 2 {
                                                let dir_slug =
                                                    workspace::slugify(&branch);
                                                Some((
                                                    parts[0].to_string(),
                                                    parts[1].to_string(),
                                                    branch,
                                                    dir_slug,
                                                    None,
                                                ))
                                            } else {
                                                None
                                            }
                                        } else {
                                            None
                                        }
                                    } else {
                                        None
                                    }
                                }
                            };

                        if let Some((owner, repo, branch, dir_slug, pr_number)) = clone_info
                        {
                            state.workspace_status =
                                Some(format!("Cloning {}/{}...", owner, repo));
                            let tx = bg_tx.clone();
                            rt.spawn(async move {
                                let result = tokio::task::spawn_blocking(move || {
                                    let ws = workspace::WorkspaceOps::new(
                                        workspace_config.dir.clone(),
                                    );
                                    let ws_dir =
                                        ws.workspace_dir(&owner, &repo, &dir_slug);

                                    if ws_dir.is_dir() {
                                        // Existing workspace -- fetch and reopen
                                        let _ =
                                            workspace::WorkspaceOps::fetch_latest(&ws_dir);
                                    } else {
                                        // Find source repo for env files + protocol detection
                                        let source = workspace::find_source_repo(
                                            &workspace_config.source_dirs,
                                            &owner,
                                            &repo,
                                        );
                                        let source_remote = source
                                            .as_ref()
                                            .and_then(|p| workspace::get_remote_url(p));
                                        let url = workspace::clone_url(
                                            source_remote.as_deref(),
                                            &owner,
                                            &repo,
                                        );

                                        // Clone
                                        ws.clone_repo(
                                            &url, &owner, &repo, &dir_slug,
                                        )?;

                                        // Copy env files
                                        if let Some(ref source_dir) = source {
                                            let _ = workspace::copy_env_files(
                                                source_dir, &ws_dir,
                                            );
                                        }

                                        // Checkout branch
                                        if let Some(pr_num) = pr_number {
                                            workspace::WorkspaceOps::checkout_pr(
                                                &ws_dir, pr_num, &branch,
                                            )?;
                                        } else {
                                            workspace::WorkspaceOps::checkout_new_branch(
                                                &ws_dir, &branch,
                                            )?;
                                        }
                                    }

                                    // Open tmux window
                                    if workspace::is_inside_tmux() {
                                        workspace::open_tmux_window(
                                            &ws_dir, &dir_slug,
                                        )?;
                                    }

                                    Ok::<String, anyhow::Error>(
                                        ws_dir.to_string_lossy().to_string(),
                                    )
                                })
                                .await
                                .map_err(|e| {
                                    anyhow::anyhow!("Task join error: {}", e)
                                })?;

                                match result {
                                    Ok(path) => {
                                        let _ =
                                            tx.send(Message::WorkspaceReady(path));
                                    }
                                    Err(e) => {
                                        let _ = tx.send(Message::WorkspaceError(
                                            e.to_string(),
                                        ));
                                    }
                                }
                                Ok::<(), anyhow::Error>(())
                            });
                        }
                    }
                }
            }
            AppEvent::Tick => {
                update(&mut state, Message::Tick);
                if let Some(url) = state.pending_open_url.take() {
                    let _ = open::that(&url);
                }
            }
            AppEvent::Resize(w, h) => {
                update(&mut state, Message::Resize(w, h));
            }
        }

        if state.should_quit {
            break;
        }
    }

    Ok(())
}

fn render(frame: &mut Frame, state: &AppState) {
    let [header_area, content_area, status_area] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(1),
    ])
    .areas(frame.area());

    // Header bar
    ui::header::render(frame, header_area, state);

    // Main content
    ui::render_content(frame, content_area, state);

    // Status bar
    let status = if let Some(ref err) = state.error {
        Line::from(Span::styled(
            format!(" Error: {} ", err),
            Style::default().fg(Color::Red),
        ))
    } else if let Some(ref ws_status) = state.workspace_status {
        Line::from(Span::styled(
            format!(" {} ", ws_status),
            Style::default().fg(Color::Yellow),
        ))
    } else {
        let mut hints = match &state.screen {
            Screen::Home => {
                " j/k scroll · h/l move · Enter open · w start work · Tab cycle filter · V list/cards · / search · n notifs · ? help · q back · Esc quit ".to_string()
            }
            Screen::RepoDetail { .. } => {
                " j/k nav · Tab next section · Enter action menu · r re-run · q back · ? help · Esc quit ".to_string()
            }
        };
        if state.screen == Screen::Home && !state.search_query.is_empty() {
            hints.push_str(&format!(" · filter: {}", state.search_query));
        }
        Line::from(Span::styled(hints, Style::default().fg(Color::DarkGray)))
    };
    frame.render_widget(Paragraph::new(status), status_area);

    // Notification overlay (on top of everything)
    if state.show_notifications {
        ui::notification_overlay::render(frame, frame.area(), state);
    }

    // Help overlay
    if state.show_help {
        render_help_overlay(frame);
    }

    // Search overlay
    if state.search_mode {
        render_search_overlay(frame, state);
    }

    // Action menu overlay
    if let Some(ref menu) = state.action_menu {
        render_action_menu(frame, frame.area(), menu);
    }

    // Branch input overlay
    if state.branch_input_mode {
        render_branch_input(frame, frame.area(), state);
    }
}

fn render_help_overlay(frame: &mut Frame) {
    let help_text = vec![
        Line::from(""),
        Line::from(Span::styled(
            "  git-mux — GitHub Dashboard",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from("  j/k ↑/↓      Navigate up/down"),
        Line::from("  h/l ←/→      Navigate left/right (card grid)"),
        Line::from("  Enter        Open repo / action menu"),
        Line::from("  w            Start work on repo (home)"),
        Line::from("  Esc          Quit"),
        Line::from("  g            Go home"),
        Line::from("  Tab          Cycle filter/section"),
        Line::from("  V            Toggle card/list view (home)"),
        Line::from("  n            Toggle notifications"),
        Line::from("  r            Refresh / re-run CI"),
        Line::from("  m            Mark notification read"),
        Line::from("  a            Mark all notifications read"),
        Line::from("  /            Search repos (home)"),
        Line::from("  ?            Toggle this help"),
        Line::from("  q            Go back / close overlay"),
        Line::from(""),
    ];

    let popup_area = centered_rect(50, 60, frame.area());
    frame.render_widget(Clear, popup_area);
    let popup = Paragraph::new(help_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Help ")
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .style(Style::default().fg(Color::White));
    frame.render_widget(popup, popup_area);
}

fn render_search_overlay(frame: &mut Frame, state: &AppState) {
    let popup_area = centered_rect_fixed(72, 5, frame.area());
    frame.render_widget(Clear, popup_area);

    let input_line = Line::from(vec![
        Span::styled(" Query: ", Style::default().fg(Color::Yellow)),
        Span::styled(&state.search_input, Style::default().fg(Color::White)),
        Span::styled("_", Style::default().fg(Color::DarkGray)),
    ]);

    let hint_line = Line::from(Span::styled(
        " Enter apply · Esc cancel ",
        Style::default().fg(Color::DarkGray),
    ));

    let popup = Paragraph::new(vec![input_line, hint_line])
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Search Repos ")
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .style(Style::default().fg(Color::White));

    frame.render_widget(popup, popup_area);
}

fn render_action_menu(frame: &mut Frame, area: Rect, menu: &app::ActionMenuState) {
    let popup_area = centered_rect_fixed(26, 4, area);
    frame.render_widget(Clear, popup_area);

    let options: Vec<&str> = match menu.context {
        app::MenuContext::Detail => vec!["Open in browser", "Start work"],
        app::MenuContext::HomeWork => vec!["Continue locally", "New branch"],
    };

    let mut lines = Vec::new();
    for (i, label) in options.iter().enumerate() {
        let (prefix, style) = if i == menu.selected {
            (" \u{25b8} ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        } else {
            ("   ", Style::default().fg(Color::White))
        };
        lines.push(Line::from(Span::styled(format!("{}{}", prefix, label), style)));
    }

    let popup = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan)),
    );
    frame.render_widget(popup, popup_area);
}

fn render_branch_input(frame: &mut Frame, area: Rect, state: &AppState) {
    let popup_area = centered_rect_fixed(60, 5, area);
    frame.render_widget(Clear, popup_area);

    let input = state.branch_input.as_deref().unwrap_or("");
    let input_line = Line::from(vec![
        Span::styled(" Branch: ", Style::default().fg(Color::Yellow)),
        Span::styled(input, Style::default().fg(Color::White)),
        Span::styled("_", Style::default().fg(Color::DarkGray)),
    ]);
    let hint_line = Line::from(Span::styled(
        " Enter confirm · Esc cancel ",
        Style::default().fg(Color::DarkGray),
    ));

    let popup = Paragraph::new(vec![input_line, hint_line]).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Start Work ")
            .border_style(Style::default().fg(Color::Cyan)),
    );
    frame.render_widget(popup, popup_area);
}

fn centered_rect_fixed(width: u16, height: u16, area: Rect) -> Rect {
    let w = width.min(area.width.saturating_sub(2)).max(10);
    let h = height.min(area.height.saturating_sub(2)).max(3);
    let x = area.x + area.width.saturating_sub(w) / 2;
    let y = area.y + area.height.saturating_sub(h) / 2;
    Rect {
        x,
        y,
        width: w,
        height: h,
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let [_, center_v, _] = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
    .areas(area);
    let [_, center, _] = Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .areas(center_v);
    center
}
