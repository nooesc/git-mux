use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};
use crate::app::{AppState, PrSection};

pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
    let is_empty = state.prs.authored.is_empty() && state.prs.review_requested.is_empty();

    if is_empty {
        let loading = if state.loading.contains(&crate::app::View::PRs) {
            "Loading pull requests..."
        } else {
            "No open pull requests"
        };
        let p = Paragraph::new(loading)
            .alignment(ratatui::layout::Alignment::Center)
            .block(Block::default().borders(Borders::ALL).title(" Pull Requests "));
        frame.render_widget(p, area);
        return;
    }

    // Filter PR lists for search
    let filtered_authored = filter_pr_list(&state.prs.authored, &state.search_query);
    let filtered_review = filter_pr_list(&state.prs.review_requested, &state.search_query);

    // Split into left (list) and right (detail) panels
    let [list_area, detail_area] = Layout::horizontal([
        Constraint::Percentage(50),
        Constraint::Percentage(50),
    ])
    .areas(area);

    // Split left panel into top (authored) and bottom (review requested)
    let [authored_area, review_area, hint_area] = Layout::vertical([
        Constraint::Percentage(50),
        Constraint::Percentage(50),
        Constraint::Length(1),
    ])
    .areas(list_area);

    // Render authored PRs section
    render_pr_list(
        frame,
        authored_area,
        " My PRs ",
        &filtered_authored,
        state.pr_section == PrSection::Authored,
        if state.pr_section == PrSection::Authored { Some(state.pr_selected) } else { None },
    );

    // Render review-requested PRs section
    render_pr_list(
        frame,
        review_area,
        " Review Requested ",
        &filtered_review,
        state.pr_section == PrSection::ReviewRequested,
        if state.pr_section == PrSection::ReviewRequested { Some(state.pr_selected) } else { None },
    );

    // Hint bar
    let hints = Paragraph::new(Line::from(vec![
        Span::styled("[Enter]", Style::default().fg(Color::Yellow)),
        Span::raw(" Open  "),
        Span::styled("[Tab]", Style::default().fg(Color::Yellow)),
        Span::raw(" Switch section"),
    ]));
    frame.render_widget(hints, hint_area);

    // Detail panel -- use filtered_prs to get the selected PR from the active section
    let filtered_active = state.filtered_prs();
    let selected_pr = filtered_active.get(state.pr_selected).copied();

    if let Some(pr) = selected_pr {
        let updated_ago = pr
            .updated_at
            .map(format_relative_time)
            .unwrap_or_else(|| "unknown".to_string());

        let created_ago = pr
            .created_at
            .map(format_relative_time)
            .unwrap_or_else(|| "unknown".to_string());

        let draft_label = if pr.draft { "Yes" } else { "No" };

        let branch_info = if pr.head_ref.is_empty() && pr.base_ref.is_empty() {
            "N/A".to_string()
        } else {
            format!("{} -> {}", pr.head_ref, pr.base_ref)
        };

        let details = vec![
            Line::from(Span::styled(
                format!("#{} {}", pr.number, pr.title),
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(vec![
                Span::raw("Repo:    "),
                Span::styled(
                    &pr.repo_full_name,
                    Style::default().fg(Color::Green),
                ),
            ]),
            Line::from(vec![
                Span::raw("Branch:  "),
                Span::styled(
                    &branch_info,
                    Style::default().fg(Color::Yellow),
                ),
            ]),
            Line::from(vec![
                Span::raw("Author:  "),
                Span::styled(
                    &pr.user,
                    Style::default().fg(Color::Magenta),
                ),
            ]),
            Line::from(vec![
                Span::raw("State:   "),
                Span::styled(
                    &pr.state,
                    Style::default().fg(Color::Green),
                ),
            ]),
            Line::from(format!("Draft:   {}", draft_label)),
            Line::from(""),
            Line::from(format!("Created: {}", created_ago)),
            Line::from(format!("Updated: {}", updated_ago)),
            Line::from(""),
            Line::from(Span::styled(
                &pr.html_url,
                Style::default().fg(Color::Blue),
            )),
        ];

        let detail = Paragraph::new(details)
            .block(Block::default().borders(Borders::ALL).title(" Details "));
        frame.render_widget(detail, detail_area);
    } else {
        let empty = Paragraph::new("No PR selected")
            .alignment(ratatui::layout::Alignment::Center)
            .block(Block::default().borders(Borders::ALL).title(" Details "));
        frame.render_widget(empty, detail_area);
    }
}

fn filter_pr_list<'a>(prs: &'a [crate::github::prs::PrInfo], query: &str) -> Vec<&'a crate::github::prs::PrInfo> {
    if query.is_empty() {
        return prs.iter().collect();
    }
    let q = query.to_lowercase();
    prs.iter().filter(|pr| {
        pr.title.to_lowercase().contains(&q)
            || pr.repo_full_name.to_lowercase().contains(&q)
            || pr.user.to_lowercase().contains(&q)
    }).collect()
}

fn render_pr_list(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    prs: &[&crate::github::prs::PrInfo],
    is_active_section: bool,
    selected: Option<usize>,
) {
    if prs.is_empty() {
        let p = Paragraph::new("  (none)")
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(title)
                    .border_style(if is_active_section {
                        Style::default().fg(Color::Cyan)
                    } else {
                        Style::default().fg(Color::DarkGray)
                    }),
            );
        frame.render_widget(p, area);
        return;
    }

    let items: Vec<ListItem> = prs
        .iter()
        .enumerate()
        .map(|(i, pr)| {
            let is_selected = selected == Some(i);
            let style = if is_selected {
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            let icon = if pr.draft { "○" } else { "●" };
            let prefix = if is_selected { ">" } else { " " };

            // Truncate title if needed for display
            let title_display = if pr.title.len() > 40 {
                format!("{}...", &pr.title[..37])
            } else {
                pr.title.clone()
            };

            ListItem::new(Line::from(vec![
                Span::styled(format!("{} ", prefix), style),
                Span::styled(
                    format!("{} ", icon),
                    if pr.draft {
                        Style::default().fg(Color::DarkGray)
                    } else {
                        Style::default().fg(Color::Green)
                    },
                ),
                Span::styled(format!("#{} ", pr.number), Style::default().fg(Color::Yellow)),
                Span::styled(title_display, style),
                Span::styled(
                    format!("  {}", pr.repo_full_name.split('/').next_back().unwrap_or("")),
                    Style::default().fg(Color::DarkGray),
                ),
            ]))
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(title)
            .border_style(if is_active_section {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default().fg(Color::DarkGray)
            }),
    );
    frame.render_widget(list, area);
}

fn format_relative_time(dt: chrono::DateTime<chrono::Utc>) -> String {
    let duration = chrono::Utc::now() - dt;
    if duration.num_days() > 0 {
        format!("{}d ago", duration.num_days())
    } else if duration.num_hours() > 0 {
        format!("{}h ago", duration.num_hours())
    } else {
        format!("{}m ago", duration.num_minutes())
    }
}
