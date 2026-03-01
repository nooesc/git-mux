mod app;
mod config;
mod event;
mod github;
mod ui;

use anyhow::Result;
use app::{AppState, Message, View, update};
use crossterm::event::KeyCode;
use event::{AppEvent, EventHandler};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Tabs};
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
    // Set initial view from config
    match config.default_view() {
        "prs" => state.active_view = View::PRs,
        "graph" => state.active_view = View::Graph,
        "notifications" => state.active_view = View::Notifications,
        "ci" => state.active_view = View::CI,
        _ => state.active_view = View::Repos,
    }

    // Poll at 200ms for UI responsiveness; auto-refresh uses timestamp checks
    let events = EventHandler::new(Duration::from_millis(200));

    // Create tokio runtime for async background tasks
    let rt = tokio::runtime::Runtime::new()?;
    let (bg_tx, bg_rx) = std::sync::mpsc::channel::<Message>();

    // Create ONE shared client, then fan out all fetches in parallel
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

            // Fan out: repos+CI, PRs, contributions, notifications — all in parallel
            let (repos_tx, prs_tx, contrib_tx, notif_tx) =
                (tx.clone(), tx.clone(), tx.clone(), tx.clone());
            let (c1, c2, c3, c4) =
                (client.clone(), client.clone(), client.clone(), client.clone());

            let repos_handle = tokio::spawn(async move {
                match c1.fetch_all_repos().await {
                    Ok(repos) => {
                        let _ = repos_tx.send(Message::ReposLoaded(repos.clone()));
                        // CI depends on repos — fetch immediately after
                        match c1.fetch_ci_runs(&repos).await {
                            Ok(runs) => { let _ = repos_tx.send(Message::CiRunsLoaded(runs)); }
                            Err(e) => { let _ = repos_tx.send(Message::Error(format!("CI: {}", e))); }
                        }
                    }
                    Err(e) => { let _ = repos_tx.send(Message::Error(format!("Repos: {}", e))); }
                }
            });

            let prs_handle = tokio::spawn(async move {
                match c2.fetch_prs().await {
                    Ok(prs) => { let _ = prs_tx.send(Message::PrsLoaded(prs)); }
                    Err(e) => { let _ = prs_tx.send(Message::Error(format!("PRs: {}", e))); }
                }
            });

            let contrib_handle = tokio::spawn(async move {
                match c3.fetch_contributions().await {
                    Ok(data) => { let _ = contrib_tx.send(Message::ContributionsLoaded(data)); }
                    Err(e) => { let _ = contrib_tx.send(Message::Error(format!("Contributions: {}", e))); }
                }
            });

            let notif_handle = tokio::spawn(async move {
                match c4.fetch_notifications().await {
                    Ok(notifs) => { let _ = notif_tx.send(Message::NotificationsLoaded(notifs)); }
                    Err(e) => { let _ = notif_tx.send(Message::Error(format!("Notifications: {}", e))); }
                }
            });

            let _ = tokio::join!(repos_handle, prs_handle, contrib_handle, notif_handle);
        });
        state.loading.insert(app::View::Repos);
        state.loading.insert(app::View::CI);
        state.loading.insert(app::View::PRs);
        state.loading.insert(app::View::Graph);
        state.loading.insert(app::View::Notifications);
    }

    loop {
        // Drain background messages
        while let Ok(msg) = bg_rx.try_recv() {
            update(&mut state, msg);
        }

        terminal.draw(|frame| render(frame, &state))?;

        match events.next()? {
            AppEvent::Key(key) => {
                // Search mode: route keys to search input
                let msg = if state.search_mode {
                    match key.code {
                        KeyCode::Esc => Some(Message::ExitSearch),
                        KeyCode::Backspace => Some(Message::SearchBackspace),
                        KeyCode::Enter => Some(Message::ExitSearch),
                        KeyCode::Char(c) => Some(Message::SearchInput(c)),
                        _ => None,
                    }
                } else if state.show_help {
                    // When help overlay is shown, only Esc and ? close it
                    match key.code {
                        KeyCode::Esc | KeyCode::Char('?') => Some(Message::ToggleHelp),
                        KeyCode::Char('q') => Some(Message::Quit),
                        _ => None,
                    }
                } else {
                    match key.code {
                        KeyCode::Char('q') => Some(Message::Quit),
                        KeyCode::Char('?') => Some(Message::ToggleHelp),
                        KeyCode::Char('/') => Some(Message::EnterSearch),
                        KeyCode::Char('1') => Some(Message::SwitchView(View::Repos)),
                        KeyCode::Char('2') => Some(Message::SwitchView(View::PRs)),
                        KeyCode::Char('3') => Some(Message::SwitchView(View::Graph)),
                        KeyCode::Char('4') => Some(Message::SwitchView(View::Notifications)),
                        KeyCode::Char('5') => Some(Message::SwitchView(View::CI)),
                        KeyCode::Tab => {
                            if state.active_view == View::PRs {
                                Some(Message::TogglePrSection)
                            } else {
                                let next = (state.active_view.index() + 1) % View::ALL.len();
                                Some(Message::SwitchView(View::ALL[next]))
                            }
                        }
                        KeyCode::BackTab => {
                            let prev = if state.active_view.index() == 0 {
                                View::ALL.len() - 1
                            } else {
                                state.active_view.index() - 1
                            };
                            Some(Message::SwitchView(View::ALL[prev]))
                        }
                        KeyCode::Char('j') | KeyCode::Down => Some(Message::Down),
                        KeyCode::Char('k') | KeyCode::Up => Some(Message::Up),
                        KeyCode::Enter => Some(Message::Select),
                        KeyCode::Esc => Some(Message::Back),
                        KeyCode::Char('r') if state.active_view == View::CI => {
                            if let Some(run) = state.ci_runs.get(state.ci_selected) {
                                let parts: Vec<&str> = run.repo_full_name.splitn(2, '/').collect();
                                if parts.len() == 2 {
                                    let owner = parts[0].to_string();
                                    let repo = parts[1].to_string();
                                    let run_id = run.id;
                                    let tx = bg_tx.clone();
                                    rt.spawn(async move {
                                        match crate::github::GitHubClient::new().await {
                                            Ok(client) => {
                                                if let Err(e) = client.rerun_workflow(&owner, &repo, run_id).await {
                                                    let _ = tx.send(Message::Error(format!("Re-run failed: {}", e)));
                                                }
                                            }
                                            Err(e) => { let _ = tx.send(Message::Error(format!("Auth failed: {}", e))); }
                                        }
                                    });
                                }
                            }
                            None
                        }
                        KeyCode::Char('r') if state.active_view != View::CI => {
                            if !state.loading.contains(&state.active_view) {
                                state.loading.insert(state.active_view);
                                spawn_refresh(state.active_view, &rt, &bg_tx, &state);
                            }
                            Some(Message::ForceRefresh)
                        }
                        KeyCode::Char('m') if state.active_view == View::Notifications => {
                            if let Some(notif) = state.notifications.get(state.notif_selected) {
                                let thread_id = notif.id.clone();
                                let tx = bg_tx.clone();
                                let tid = thread_id.clone();
                                rt.spawn(async move {
                                    match crate::github::GitHubClient::new().await {
                                        Ok(client) => {
                                            if let Err(e) = client.mark_notification_read(&tid).await {
                                                let _ = tx.send(Message::Error(format!("Failed to mark read: {}", e)));
                                            }
                                        }
                                        Err(e) => { let _ = tx.send(Message::Error(format!("Auth failed: {}", e))); }
                                    }
                                });
                                Some(Message::MarkNotifRead(thread_id))
                            } else {
                                None
                            }
                        }
                        _ => None,
                    }
                };

                if let Some(msg) = msg {
                    let is_switch = matches!(msg, Message::SwitchView(_));
                    update(&mut state, msg);

                    // After switching views, check if the new view is stale
                    if is_switch {
                        let stale = state.last_refresh.get(&state.active_view)
                            .map(|t| t.elapsed().as_secs() > config.general.refresh_interval_secs)
                            .unwrap_or(true);

                        if stale && !state.loading.contains(&state.active_view) {
                            state.loading.insert(state.active_view);
                            spawn_refresh(state.active_view, &rt, &bg_tx, &state);
                        }
                    }

                    if let Some(url) = state.pending_open_url.take() {
                        let _ = open::that(&url);
                    }
                }
            }
            AppEvent::Tick => {
                // Check if active view needs refresh
                let stale = state.last_refresh.get(&state.active_view)
                    .map(|t| t.elapsed().as_secs() > config.general.refresh_interval_secs)
                    .unwrap_or(true);

                if stale && !state.loading.contains(&state.active_view) {
                    state.loading.insert(state.active_view);
                    spawn_refresh(state.active_view, &rt, &bg_tx, &state);
                }

                update(&mut state, Message::Tick);
                if let Some(url) = state.pending_open_url.take() {
                    let _ = open::that(&url);
                }
            }
        }

        if state.should_quit {
            break;
        }
    }

    Ok(())
}

fn spawn_refresh(
    view: View,
    rt: &tokio::runtime::Runtime,
    tx: &std::sync::mpsc::Sender<Message>,
    state: &AppState,
) {
    let tx = tx.clone();
    match view {
        View::Repos => {
            rt.spawn(async move {
                match crate::github::GitHubClient::new().await {
                    Ok(client) => match client.fetch_all_repos().await {
                        Ok(repos) => { let _ = tx.send(Message::ReposLoaded(repos)); }
                        Err(e) => { let _ = tx.send(Message::Error(format!("Repos: {}", e))); }
                    },
                    Err(e) => { let _ = tx.send(Message::Error(format!("Auth: {}", e))); }
                }
            });
        }
        View::PRs => {
            rt.spawn(async move {
                match crate::github::GitHubClient::new().await {
                    Ok(client) => match client.fetch_prs().await {
                        Ok(prs) => { let _ = tx.send(Message::PrsLoaded(prs)); }
                        Err(e) => { let _ = tx.send(Message::Error(format!("PRs: {}", e))); }
                    },
                    Err(e) => { let _ = tx.send(Message::Error(format!("Auth: {}", e))); }
                }
            });
        }
        View::Graph => {
            rt.spawn(async move {
                match crate::github::GitHubClient::new().await {
                    Ok(client) => match client.fetch_contributions().await {
                        Ok(data) => { let _ = tx.send(Message::ContributionsLoaded(data)); }
                        Err(e) => { let _ = tx.send(Message::Error(format!("Contributions: {}", e))); }
                    },
                    Err(e) => { let _ = tx.send(Message::Error(format!("Auth: {}", e))); }
                }
            });
        }
        View::Notifications => {
            rt.spawn(async move {
                match crate::github::GitHubClient::new().await {
                    Ok(client) => match client.fetch_notifications().await {
                        Ok(notifs) => { let _ = tx.send(Message::NotificationsLoaded(notifs)); }
                        Err(e) => { let _ = tx.send(Message::Error(format!("Notifications: {}", e))); }
                    },
                    Err(e) => { let _ = tx.send(Message::Error(format!("Auth: {}", e))); }
                }
            });
        }
        View::CI => {
            let repos = state.repos.clone();
            rt.spawn(async move {
                match crate::github::GitHubClient::new().await {
                    Ok(client) => match client.fetch_ci_runs(&repos).await {
                        Ok(runs) => { let _ = tx.send(Message::CiRunsLoaded(runs)); }
                        Err(e) => { let _ = tx.send(Message::Error(format!("CI: {}", e))); }
                    },
                    Err(e) => { let _ = tx.send(Message::Error(format!("Auth: {}", e))); }
                }
            });
        }
    }
}

// Note: spawn_refresh still creates new clients per refresh. This is fine for
// periodic refreshes (every 60s), the startup optimization is what matters most.

fn render(frame: &mut Frame, state: &AppState) {
    let [tab_area, content_area, status_area] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Fill(1),
        Constraint::Length(1),
    ])
    .areas(frame.area());

    // Tab bar
    let tab_titles: Vec<Line> = View::ALL
        .iter()
        .enumerate()
        .map(|(i, v)| Line::from(format!(" {} {} ", i + 1, v.label())))
        .collect();

    let tabs = Tabs::new(tab_titles)
        .block(Block::default().borders(Borders::ALL).title(" ghd "))
        .select(state.active_view.index())
        .highlight_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );

    frame.render_widget(tabs, tab_area);

    // Content area
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
        Line::from(Span::styled(
            " q:quit  r:refresh  /:search  ?:help  1-5:switch view ",
            Style::default().fg(Color::DarkGray),
        ))
    };

    frame.render_widget(Paragraph::new(status), status_area);

    // Help overlay
    if state.show_help {
        let help_text = vec![
            Line::from("ghd -- GitHub Dashboard"),
            Line::from(""),
            Line::from("1-5        Switch view"),
            Line::from("Tab        Next view / Toggle PR section"),
            Line::from("Shift+Tab  Previous view"),
            Line::from("j/k        Navigate"),
            Line::from("Enter      Open in browser"),
            Line::from("r          Refresh / Re-run (CI)"),
            Line::from("m          Mark notification read"),
            Line::from("/          Search"),
            Line::from("?          Toggle this help"),
            Line::from("q          Quit"),
            Line::from(""),
            Line::from("Esc        Close help / Cancel"),
        ];
        let popup_area = centered_rect(50, 60, frame.area());
        frame.render_widget(Clear, popup_area);
        let popup = Paragraph::new(help_text)
            .block(Block::default().borders(Borders::ALL).title(" Help "))
            .style(Style::default().fg(Color::White));
        frame.render_widget(popup, popup_area);
    }
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
