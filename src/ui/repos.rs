use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};
use crate::app::AppState;

pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
    let filtered = state.filtered_repos();

    if filtered.is_empty() {
        let loading = if state.loading.contains(&crate::app::View::Repos) {
            "Loading repos..."
        } else if !state.search_query.is_empty() {
            "No matching repos"
        } else {
            "No repos found"
        };
        let p = Paragraph::new(loading)
            .alignment(ratatui::layout::Alignment::Center)
            .block(Block::default().borders(Borders::ALL).title(" Repos "));
        frame.render_widget(p, area);
        return;
    }

    let [list_area, detail_area] = Layout::horizontal([
        Constraint::Percentage(40),
        Constraint::Percentage(60),
    ])
    .areas(area);

    // Left panel: repo list (filtered)
    let items: Vec<ListItem> = filtered
        .iter()
        .enumerate()
        .map(|(i, repo)| {
            let style = if i == state.repo_selected {
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            let prefix = if i == state.repo_selected { ">" } else { " " };
            let private_marker = if repo.is_private { " [private]" } else { "" };

            ListItem::new(Line::from(vec![
                Span::styled(format!("{} ", prefix), style),
                Span::styled(&repo.full_name, style),
                Span::styled(
                    format!("  *{}{}", repo.stargazers_count, private_marker),
                    Style::default().fg(Color::Yellow),
                ),
            ]))
        })
        .collect();

    let title = if state.search_query.is_empty() {
        " Repos ".to_string()
    } else {
        format!(" Repos ({} matches) ", filtered.len())
    };

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title));
    frame.render_widget(list, list_area);

    // Right panel: selected repo details
    if let Some(repo) = filtered.get(state.repo_selected) {
        let pushed_ago = repo
            .pushed_at
            .map(|dt| {
                let duration = chrono::Utc::now() - dt;
                if duration.num_days() > 0 {
                    format!("{}d ago", duration.num_days())
                } else if duration.num_hours() > 0 {
                    format!("{}h ago", duration.num_hours())
                } else {
                    format!("{}m ago", duration.num_minutes())
                }
            })
            .unwrap_or_else(|| "unknown".to_string());

        let details = vec![
            Line::from(Span::styled(
                &repo.full_name,
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(vec![
                Span::raw("Language: "),
                Span::styled(
                    repo.language.as_deref().unwrap_or("--"),
                    Style::default().fg(Color::Green),
                ),
            ]),
            Line::from(vec![
                Span::styled(
                    format!("* {}", repo.stargazers_count),
                    Style::default().fg(Color::Yellow),
                ),
                Span::raw("  "),
                Span::raw(format!("Forks: {}", repo.forks_count)),
            ]),
            Line::from(format!("Issues: {}", repo.open_issues_count)),
            Line::from(format!("Last push: {}", pushed_ago)),
            Line::from(""),
            Line::from(
                repo.description
                    .as_deref()
                    .unwrap_or("No description"),
            ),
            Line::from(""),
            Line::from(Span::styled(
                "[Enter] Open in browser",
                Style::default().fg(Color::DarkGray),
            )),
        ];

        let detail = Paragraph::new(details)
            .block(Block::default().borders(Borders::ALL).title(" Details "));
        frame.render_widget(detail, detail_area);
    }
}
