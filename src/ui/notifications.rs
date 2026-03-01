use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};
use crate::app::AppState;

pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
    if state.notifications.is_empty() {
        let loading = if state.loading.contains(&crate::app::View::Notifications) {
            "Loading notifications..."
        } else {
            "No notifications"
        };
        let p = Paragraph::new(loading)
            .alignment(ratatui::layout::Alignment::Center)
            .block(Block::default().borders(Borders::ALL).title(" Notifications "));
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

    // Build notification list items
    let items: Vec<ListItem> = state
        .notifications
        .iter()
        .enumerate()
        .map(|(i, notif)| {
            let is_selected = i == state.notif_selected;
            let base_style = if is_selected {
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else if notif.unread {
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
            };

            let prefix = if is_selected { ">" } else { " " };
            let type_icon = type_icon(&notif.subject_type);

            // Truncate title for display
            let title_display = if notif.subject_title.len() > 40 {
                format!("{}...", &notif.subject_title[..37])
            } else {
                notif.subject_title.clone()
            };

            let unread_marker = if notif.unread { "●" } else { " " };

            ListItem::new(Line::from(vec![
                Span::styled(format!("{} ", prefix), base_style),
                Span::styled(
                    format!("{} ", unread_marker),
                    if notif.unread {
                        Style::default().fg(Color::Blue)
                    } else {
                        Style::default().fg(Color::DarkGray)
                    },
                ),
                Span::styled(
                    format!("{} ", type_icon),
                    type_color(&notif.subject_type),
                ),
                Span::styled(title_display, base_style),
                Span::styled(
                    format!("  {}", notif.repo_full_name.split('/').last().unwrap_or("")),
                    Style::default().fg(Color::DarkGray),
                ),
            ]))
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(" Notifications "));
    frame.render_widget(list, items_area);

    // Hint bar
    let hints = Paragraph::new(Line::from(vec![
        Span::styled("[Enter]", Style::default().fg(Color::Yellow)),
        Span::raw(" Open  "),
        Span::styled("[m]", Style::default().fg(Color::Yellow)),
        Span::raw(" Mark read"),
    ]));
    frame.render_widget(hints, hint_area);

    // Detail panel for selected notification
    if let Some(notif) = state.notifications.get(state.notif_selected) {
        let updated_ago = notif
            .updated_at
            .map(|dt| format_relative_time(dt))
            .unwrap_or_else(|| "unknown".to_string());

        let reason_display = format_reason(&notif.reason);
        let unread_label = if notif.unread { "Yes" } else { "No" };

        let details = vec![
            Line::from(Span::styled(
                &notif.subject_title,
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(vec![
                Span::raw("Type:    "),
                Span::styled(
                    &notif.subject_type,
                    type_color(&notif.subject_type),
                ),
            ]),
            Line::from(vec![
                Span::raw("Repo:    "),
                Span::styled(
                    &notif.repo_full_name,
                    Style::default().fg(Color::Green),
                ),
            ]),
            Line::from(vec![
                Span::raw("Reason:  "),
                Span::styled(
                    reason_display,
                    Style::default().fg(Color::Yellow),
                ),
            ]),
            Line::from(format!("Unread:  {}", unread_label)),
            Line::from(format!("Updated: {}", updated_ago)),
            Line::from(""),
            Line::from(Span::styled(
                notif.url.as_deref().unwrap_or("(no url)"),
                Style::default().fg(Color::Blue),
            )),
        ];

        let detail = Paragraph::new(details)
            .block(Block::default().borders(Borders::ALL).title(" Details "));
        frame.render_widget(detail, detail_area);
    } else {
        let empty = Paragraph::new("No notification selected")
            .alignment(ratatui::layout::Alignment::Center)
            .block(Block::default().borders(Borders::ALL).title(" Details "));
        frame.render_widget(empty, detail_area);
    }
}

fn type_icon(subject_type: &str) -> &'static str {
    match subject_type {
        "PullRequest" => "PR",
        "Issue" => "IS",
        "Release" => "RE",
        "CheckSuite" => "CI",
        "Discussion" => "DI",
        "Commit" => "CM",
        _ => "??",
    }
}

fn type_color(subject_type: &str) -> Style {
    match subject_type {
        "PullRequest" => Style::default().fg(Color::Green),
        "Issue" => Style::default().fg(Color::Yellow),
        "Release" => Style::default().fg(Color::Magenta),
        "CheckSuite" => Style::default().fg(Color::Red),
        "Discussion" => Style::default().fg(Color::Cyan),
        "Commit" => Style::default().fg(Color::Blue),
        _ => Style::default().fg(Color::DarkGray),
    }
}

fn format_reason(reason: &str) -> &str {
    match reason {
        "assign" => "Assigned",
        "author" => "Author",
        "comment" => "Comment",
        "ci_activity" => "CI Activity",
        "invitation" => "Invitation",
        "manual" => "Manual",
        "mention" => "Mentioned",
        "review_requested" => "Review Requested",
        "security_alert" => "Security Alert",
        "state_change" => "State Changed",
        "subscribed" => "Subscribed",
        "team_mention" => "Team Mentioned",
        _ => reason,
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
