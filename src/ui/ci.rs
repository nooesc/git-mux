use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};
use crate::app::AppState;

pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
    if state.ci_runs.is_empty() {
        let loading = if state.loading.contains(&crate::app::View::CI) {
            "Loading CI runs..."
        } else {
            "No CI runs found"
        };
        let p = Paragraph::new(loading)
            .alignment(ratatui::layout::Alignment::Center)
            .block(Block::default().borders(Borders::ALL).title(" CI Runs "));
        frame.render_widget(p, area);
        return;
    }

    let [list_area, detail_area] = Layout::horizontal([
        Constraint::Percentage(50),
        Constraint::Percentage(50),
    ])
    .areas(area);

    // Split list area to include hint bar at bottom
    let [items_area, hint_area] = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(1),
    ])
    .areas(list_area);

    // Build CI run list items, grouped by repo
    let items: Vec<ListItem> = state
        .ci_runs
        .iter()
        .enumerate()
        .map(|(i, run)| {
            let is_selected = i == state.ci_selected;
            let base_style = if is_selected {
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            let prefix = if is_selected { ">" } else { " " };
            let (icon, icon_color) = status_icon_and_color(&run.status, &run.conclusion);

            let time_ago = run
                .created_at
                .map(|dt| format_relative_time(dt))
                .unwrap_or_default();

            let duration_str = run
                .duration_secs
                .map(|s| format_duration(s))
                .unwrap_or_default();

            // Truncate workflow name for display
            let name_display = if run.name.len() > 25 {
                format!("{}...", &run.name[..22])
            } else {
                run.name.clone()
            };

            // Truncate branch name for display
            let branch_display = if run.head_branch.len() > 15 {
                format!("{}...", &run.head_branch[..12])
            } else {
                run.head_branch.clone()
            };

            let repo_short = run.repo_full_name.split('/').last().unwrap_or("");

            ListItem::new(Line::from(vec![
                Span::styled(format!("{} ", prefix), base_style),
                Span::styled(format!("{} ", icon), Style::default().fg(icon_color)),
                Span::styled(format!("{} ", name_display), base_style),
                Span::styled(
                    format!("{} ", branch_display),
                    Style::default().fg(Color::Magenta),
                ),
                Span::styled(
                    format!("{} ", repo_short),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(time_ago, Style::default().fg(Color::DarkGray)),
                if !duration_str.is_empty() {
                    Span::styled(
                        format!(" ({})", duration_str),
                        Style::default().fg(Color::DarkGray),
                    )
                } else {
                    Span::raw("")
                },
            ]))
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(" CI Runs "));
    frame.render_widget(list, items_area);

    // Hint bar
    let hints = Paragraph::new(Line::from(vec![
        Span::styled("[Enter]", Style::default().fg(Color::Yellow)),
        Span::raw(" Open logs  "),
        Span::styled("[r]", Style::default().fg(Color::Yellow)),
        Span::raw(" Re-run"),
    ]));
    frame.render_widget(hints, hint_area);

    // Detail panel for selected run
    if let Some(run) = state.ci_runs.get(state.ci_selected) {
        let (icon, icon_color) = status_icon_and_color(&run.status, &run.conclusion);
        let conclusion_display = run
            .conclusion
            .as_deref()
            .unwrap_or("--");

        let created_ago = run
            .created_at
            .map(|dt| format_relative_time(dt))
            .unwrap_or_else(|| "unknown".to_string());

        let duration_display = run
            .duration_secs
            .map(|s| format_duration(s))
            .unwrap_or_else(|| "--".to_string());

        let details = vec![
            Line::from(Span::styled(
                &run.name,
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(vec![
                Span::raw("Repo:       "),
                Span::styled(
                    &run.repo_full_name,
                    Style::default().fg(Color::Green),
                ),
            ]),
            Line::from(vec![
                Span::raw("Branch:     "),
                Span::styled(
                    &run.head_branch,
                    Style::default().fg(Color::Magenta),
                ),
            ]),
            Line::from(vec![
                Span::raw("Status:     "),
                Span::styled(
                    format!("{} {}", icon, &run.status),
                    Style::default().fg(icon_color),
                ),
            ]),
            Line::from(vec![
                Span::raw("Conclusion: "),
                Span::styled(
                    conclusion_display,
                    Style::default().fg(conclusion_color(conclusion_display)),
                ),
            ]),
            Line::from(format!("Duration:   {}", duration_display)),
            Line::from(format!("Created:    {}", created_ago)),
            Line::from(""),
            Line::from(Span::styled(
                &run.html_url,
                Style::default().fg(Color::Blue),
            )),
        ];

        let detail = Paragraph::new(details)
            .block(Block::default().borders(Borders::ALL).title(" Details "));
        frame.render_widget(detail, detail_area);
    } else {
        let empty = Paragraph::new("No CI run selected")
            .alignment(ratatui::layout::Alignment::Center)
            .block(Block::default().borders(Borders::ALL).title(" Details "));
        frame.render_widget(empty, detail_area);
    }
}

fn status_icon_and_color(status: &str, conclusion: &Option<String>) -> (&'static str, Color) {
    match status {
        "completed" => match conclusion.as_deref() {
            Some("success") => ("v", Color::Green),
            Some("failure") | Some("timed_out") => ("x", Color::Red),
            Some("cancelled") | Some("skipped") => ("-", Color::Yellow),
            _ => ("?", Color::DarkGray),
        },
        "in_progress" => ("~", Color::Yellow),
        "queued" | "waiting" | "pending" => (".", Color::Yellow),
        _ => ("?", Color::DarkGray),
    }
}

fn conclusion_color(conclusion: &str) -> Color {
    match conclusion {
        "success" => Color::Green,
        "failure" | "timed_out" => Color::Red,
        "cancelled" | "skipped" => Color::Yellow,
        _ => Color::DarkGray,
    }
}

fn format_duration(secs: i64) -> String {
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        format!("{}m{}s", secs / 60, secs % 60)
    } else {
        format!("{}h{}m", secs / 3600, (secs % 3600) / 60)
    }
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
