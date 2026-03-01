mod app;
mod config;
mod event;
mod github;
mod ui;

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

    // ── Startup fetch: repos, contributions, notifications, avatar ──
    {
        let tx = bg_tx.clone();
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
                        let _ = tx_avatar.send(Message::AvatarLoaded(buf.into_inner()));
                    }
                }
            });

            // Fetch repos, contributions, notifications in parallel
            let (tx_repos, tx_contrib, tx_notif) = (tx.clone(), tx.clone(), tx.clone());
            let (c1, c2, c3) = (client.clone(), client.clone(), client.clone());

            let repos_handle = tokio::spawn(async move {
                match c1.fetch_all_repos().await {
                    Ok(repos) => { let _ = tx_repos.send(Message::ReposLoaded(repos)); }
                    Err(e) => { let _ = tx_repos.send(Message::Error(format!("Repos: {}", e))); }
                }
            });

            let contrib_handle = tokio::spawn(async move {
                match c2.fetch_contributions().await {
                    Ok(data) => { let _ = tx_contrib.send(Message::ContributionsLoaded(data)); }
                    Err(e) => { let _ = tx_contrib.send(Message::Error(format!("Contributions: {}", e))); }
                }
            });

            let notif_handle = tokio::spawn(async move {
                match c3.fetch_notifications().await {
                    Ok(notifs) => { let _ = tx_notif.send(Message::NotificationsLoaded(notifs)); }
                    Err(e) => { let _ = tx_notif.send(Message::Error(format!("Notifications: {}", e))); }
                }
            });

            let _ = tokio::join!(avatar_handle, repos_handle, contrib_handle, notif_handle);
        });

        state.loading.insert("repos".to_string());
        state.loading.insert("contributions".to_string());
        state.loading.insert("avatar".to_string());
        state.loading.insert("notifications".to_string());
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
                        KeyCode::Esc => Some(Message::ExitSearch),
                        KeyCode::Enter => Some(Message::ExitSearch),
                        KeyCode::Backspace => Some(Message::SearchBackspace),
                        KeyCode::Char(c) => Some(Message::SearchInput(c)),
                        _ => None,
                    }
                } else if state.show_help {
                    match key.code {
                        KeyCode::Esc | KeyCode::Char('?') => Some(Message::ToggleHelp),
                        KeyCode::Char('q') => Some(Message::Quit),
                        _ => None,
                    }
                } else if state.show_notifications {
                    match key.code {
                        KeyCode::Esc => Some(Message::Back),
                        KeyCode::Char('n') => Some(Message::ToggleNotifications),
                        KeyCode::Char('q') => Some(Message::Quit),
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
                            let ids: Vec<String> = state.notifications.iter()
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
                } else {
                    match key.code {
                        KeyCode::Char('q') => Some(Message::Quit),
                        KeyCode::Char('?') => Some(Message::ToggleHelp),
                        KeyCode::Char('/') => Some(Message::EnterSearch),
                        KeyCode::Char('n') => Some(Message::ToggleNotifications),

                        KeyCode::Char('j') | KeyCode::Down => Some(Message::Down),
                        KeyCode::Char('k') | KeyCode::Up => Some(Message::Up),
                        KeyCode::Char('h') | KeyCode::Left => Some(Message::Left),
                        KeyCode::Char('l') | KeyCode::Right => Some(Message::Right),

                        KeyCode::Enter => {
                            if state.screen == Screen::Home
                                && state.home_focus == app::HomeFocus::Repos
                                && let Some(repo) = state.selected_repo() {
                                    let repo_name = repo.full_name.clone();
                                    let tx = bg_tx.clone();
                                    rt.spawn(async move {
                                        let parts: Vec<&str> = repo_name.splitn(2, '/').collect();
                                        if parts.len() == 2 {
                                            let (owner, name) = (parts[0], parts[1]);
                                            if let Ok(client) = crate::github::GitHubClient::from_token() {
                                                let (prs, issues, ci) = tokio::join!(
                                                    client.fetch_repo_prs(owner, name),
                                                    client.fetch_repo_issues(owner, name),
                                                    client.fetch_repo_ci(owner, name),
                                                );
                                                let _ = tx.send(Message::RepoDetailLoaded {
                                                    repo: format!("{}/{}", owner, name),
                                                    prs: prs.unwrap_or_default(),
                                                    issues: issues.unwrap_or_default(),
                                                    ci: ci.unwrap_or_default(),
                                                });
                                            }
                                        }
                                    });
                                }
                            Some(Message::Select)
                        }

                        KeyCode::Esc => Some(Message::Back),

                        KeyCode::Char('g') => {
                            Some(Message::GoHome)
                        }

                        KeyCode::Tab => {
                            if matches!(state.screen, Screen::RepoDetail { .. }) {
                                Some(Message::CycleSection)
                            } else {
                                None
                            }
                        }

                        KeyCode::Char('v') | KeyCode::Char('V') => Some(Message::ToggleViewMode),

                        KeyCode::Char('r') => {
                            match &state.screen {
                                Screen::RepoDetail { section: crate::app::RepoSection::CI, .. } => {
                                    let items = state.filtered_detail_items();
                                    if let Some(crate::app::DetailItem::Ci(run)) = items.get(state.detail_selected) {
                                        let parts: Vec<&str> = run.repo_full_name.splitn(2, '/').collect();
                                        if parts.len() == 2 {
                                            let owner = parts[0].to_string();
                                            let repo = parts[1].to_string();
                                            let run_id = run.id;
                                            let tx = bg_tx.clone();
                                            rt.spawn(async move {
                                                if let Ok(client) = crate::github::GitHubClient::from_token()
                                                    && let Err(e) = client.rerun_workflow(&owner, &repo, run_id).await {
                                                    let _ = tx.send(Message::Error(format!("Re-run failed: {}", e)));
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
                                            if let Ok(client) = crate::github::GitHubClient::from_token()
                                                && let Ok(repos) = client.fetch_all_repos().await {
                                                let _ = tx.send(Message::ReposLoaded(repos));
                                            }
                                        });
                                    }
                                    Some(Message::ForceRefresh)
                                }
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
    ]).areas(frame.area());

    // Header bar
    ui::header::render(frame, header_area, state);

    // Main content
    ui::render_content(frame, content_area, state);

    // Status bar
    let status = if state.search_mode {
        Line::from(vec![
            Span::styled(" / ", Style::default().fg(Color::Yellow)),
            Span::styled(&state.search_query, Style::default().fg(Color::White)),
            Span::styled("_", Style::default().fg(Color::DarkGray)),
        ])
    } else if let Some(ref err) = state.error {
        Line::from(Span::styled(
            format!(" Error: {} ", err),
            Style::default().fg(Color::Red),
        ))
    } else {
        let hints = match &state.screen {
            Screen::Home => " j/k scroll · h/l move · Enter open · v toggle view · / search · n notifs · ? help · q quit ",
            Screen::RepoDetail { .. } => " j/k nav · Tab section · Enter open · r re-run · Esc back · / search · ? help · q quit ",
        };
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
}

fn render_help_overlay(frame: &mut Frame) {
    let help_text = vec![
        Line::from(""),
        Line::from(Span::styled("  ghd — GitHub Dashboard", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))),
        Line::from(""),
        Line::from("  j/k ↑/↓      Navigate up/down"),
        Line::from("  h/l ←/→      Navigate left/right (card grid)"),
        Line::from("  Enter        Open repo / open in browser"),
        Line::from("  Esc          Go back / close overlay"),
        Line::from("  g            Go home"),
        Line::from("  Tab          Cycle sections (repo detail)"),
        Line::from("  v            Toggle card/list view"),
        Line::from("  n            Toggle notifications"),
        Line::from("  r            Refresh / re-run CI"),
        Line::from("  m            Mark notification read"),
        Line::from("  a            Mark all notifications read"),
        Line::from("  /            Search/filter"),
        Line::from("  ?            Toggle this help"),
        Line::from("  q            Quit"),
        Line::from(""),
    ];

    let popup_area = centered_rect(50, 60, frame.area());
    frame.render_widget(Clear, popup_area);
    let popup = Paragraph::new(help_text)
        .block(Block::default().borders(Borders::ALL).title(" Help ").border_style(Style::default().fg(Color::Cyan)))
        .style(Style::default().fg(Color::White));
    frame.render_widget(popup, popup_area);
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
