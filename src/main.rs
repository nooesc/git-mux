mod app;
mod event;
mod github;
mod ui;

use anyhow::Result;
use app::{AppState, Message, View, update};
use crossterm::event::KeyCode;
use event::{AppEvent, EventHandler};
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Tabs};
use ratatui::{DefaultTerminal, Frame};
use std::time::Duration;

fn main() -> Result<()> {
    let mut terminal = ratatui::init();
    let result = run(&mut terminal);
    ratatui::restore();
    result
}

fn run(terminal: &mut DefaultTerminal) -> Result<()> {
    let mut state = AppState::new();
    let events = EventHandler::new(Duration::from_secs(1));

    // Create tokio runtime for async background tasks
    let rt = tokio::runtime::Runtime::new()?;
    let (bg_tx, bg_rx) = std::sync::mpsc::channel::<Message>();

    // Spawn initial data fetch
    {
        let tx = bg_tx.clone();
        rt.spawn(async move {
            match crate::github::GitHubClient::new().await {
                Ok(client) => {
                    match client.fetch_all_repos().await {
                        Ok(repos) => { let _ = tx.send(Message::ReposLoaded(repos)); }
                        Err(e) => { let _ = tx.send(Message::Error(format!("Failed to fetch repos: {}", e))); }
                    }
                }
                Err(e) => { let _ = tx.send(Message::Error(format!("Auth failed: {}", e))); }
            }
        });
        state.loading.insert(app::View::Repos);
    }

    // Spawn PR fetch
    {
        let tx = bg_tx.clone();
        rt.spawn(async move {
            match crate::github::GitHubClient::new().await {
                Ok(client) => {
                    match client.fetch_prs().await {
                        Ok(prs) => { let _ = tx.send(Message::PrsLoaded(prs)); }
                        Err(e) => { let _ = tx.send(Message::Error(format!("Failed to fetch PRs: {}", e))); }
                    }
                }
                Err(e) => { let _ = tx.send(Message::Error(format!("Auth failed: {}", e))); }
            }
        });
        state.loading.insert(app::View::PRs);
    }

    // Spawn contributions fetch
    {
        let tx = bg_tx.clone();
        rt.spawn(async move {
            match crate::github::GitHubClient::new().await {
                Ok(client) => {
                    match client.fetch_contributions().await {
                        Ok(data) => { let _ = tx.send(Message::ContributionsLoaded(data)); }
                        Err(e) => { let _ = tx.send(Message::Error(format!("Failed to fetch contributions: {}", e))); }
                    }
                }
                Err(e) => { let _ = tx.send(Message::Error(format!("Auth failed: {}", e))); }
            }
        });
        state.loading.insert(app::View::Graph);
    }

    // Spawn notifications fetch
    {
        let tx = bg_tx.clone();
        rt.spawn(async move {
            match crate::github::GitHubClient::new().await {
                Ok(client) => {
                    match client.fetch_notifications().await {
                        Ok(notifs) => { let _ = tx.send(Message::NotificationsLoaded(notifs)); }
                        Err(e) => { let _ = tx.send(Message::Error(format!("Failed to fetch notifications: {}", e))); }
                    }
                }
                Err(e) => { let _ = tx.send(Message::Error(format!("Auth failed: {}", e))); }
            }
        });
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
                let msg = match key.code {
                    KeyCode::Char('q') => Some(Message::Quit),
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
                    KeyCode::Char('r') => Some(Message::ForceRefresh),
                    KeyCode::Char('j') | KeyCode::Down => Some(Message::Down),
                    KeyCode::Char('k') | KeyCode::Up => Some(Message::Up),
                    KeyCode::Enter => Some(Message::Select),
                    KeyCode::Esc => Some(Message::Back),
                    KeyCode::Char('m') if state.active_view == View::Notifications => {
                        if let Some(notif) = state.notifications.get(state.notif_selected) {
                            let thread_id = notif.id.clone();
                            // Spawn background task to mark as read via API
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
        }

        if state.should_quit {
            break;
        }
    }

    Ok(())
}

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
    let status = if let Some(ref err) = state.error {
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
}
