use chrono::Utc;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::{AppState, RepoSection};

pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
    let (repo_full_name, section) = match &state.screen {
        crate::app::Screen::RepoDetail { repo_full_name, section } => (repo_full_name, *section),
        _ => return,
    };

    let [header_area, tabs_area, content_area] = Layout::vertical([
        Constraint::Length(4),
        Constraint::Length(2),
        Constraint::Fill(1),
    ]).areas(area);

    // ── Repo header ──
    let repo = state.repos.iter().find(|r| r.full_name == *repo_full_name);

    let mut header_lines = vec![
        Line::from(vec![
            Span::styled(
                format!("  {}", repo_full_name),
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(
                if repo.is_some_and(|r| r.is_private) { "🔒 Private" } else { "Public" },
                Style::default().fg(Color::DarkGray),
            ),
        ]),
    ];

    if let Some(r) = repo {
        if let Some(ref desc) = r.description {
            header_lines.push(Line::from(Span::styled(
                format!("  {}", desc),
                Style::default().fg(Color::DarkGray),
            )));
        }
        header_lines.push(Line::from(vec![
            Span::styled(
                format!("  {} ", r.language.as_deref().unwrap_or("")),
                Style::default().fg(Color::Magenta),
            ),
            Span::styled(format!("· ★ {} ", r.stargazers_count), Style::default().fg(Color::Yellow)),
            Span::styled(format!("· ⎚ {} ", r.forks_count), Style::default().fg(Color::DarkGray)),
            Span::styled(format!("· ⚠ {} ", r.open_issues_count), Style::default().fg(Color::DarkGray)),
            Span::raw("· "),
            Span::styled(
                r.pushed_at.map(|dt| {
                    let ago = Utc::now().signed_duration_since(dt);
                    if ago.num_hours() < 1 { format!("pushed {}m ago", ago.num_minutes()) }
                    else if ago.num_hours() < 24 { format!("pushed {}h ago", ago.num_hours()) }
                    else { format!("pushed {}d ago", ago.num_days()) }
                }).unwrap_or_default(),
                Style::default().fg(Color::DarkGray),
            ),
        ]));
    }

    frame.render_widget(Paragraph::new(header_lines), header_area);

    // ── Section tabs ──
    let pr_count = state.repo_prs.len();
    let issue_count = state.repo_issues.len();
    let ci_count = state.repo_ci.len();

    let tabs_line = Line::from(vec![
        Span::raw("  "),
        section_tab("PRs", pr_count, section == RepoSection::PRs),
        Span::raw("    "),
        section_tab("Issues", issue_count, section == RepoSection::Issues),
        Span::raw("    "),
        section_tab("CI", ci_count, section == RepoSection::CI),
    ]);

    let underline = Line::from(vec![
        Span::raw("  "),
        section_underline(section == RepoSection::PRs),
        Span::raw("    "),
        section_underline(section == RepoSection::Issues),
        Span::raw("    "),
        section_underline(section == RepoSection::CI),
    ]);

    frame.render_widget(Paragraph::new(vec![tabs_line, underline]), tabs_area);

    // ── Content ──
    if state.loading.contains("repo_detail") {
        let p = Paragraph::new("  Loading...")
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(p, content_area);
        return;
    }

    match section {
        RepoSection::PRs => render_pr_list(frame, content_area, state),
        RepoSection::Issues => render_issue_list(frame, content_area, state),
        RepoSection::CI => render_ci_list(frame, content_area, state),
    }
}

fn section_tab(label: &str, count: usize, active: bool) -> Span<'static> {
    let text = format!("{} ({})", label, count);
    if active {
        Span::styled(text, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
    } else {
        Span::styled(text, Style::default().fg(Color::DarkGray))
    }
}

fn section_underline(active: bool) -> Span<'static> {
    if active {
        Span::styled("━━━━━━━", Style::default().fg(Color::Cyan))
    } else {
        Span::styled("───────", Style::default().fg(Color::DarkGray))
    }
}

fn render_pr_list(frame: &mut Frame, area: Rect, state: &AppState) {
    let items = state.filtered_detail_items();
    if items.is_empty() {
        frame.render_widget(
            Paragraph::new("  No pull requests").style(Style::default().fg(Color::DarkGray)),
            area,
        );
        return;
    }

    let mut lines: Vec<Line> = Vec::new();
    for (idx, item) in items.iter().enumerate() {
        if let crate::app::DetailItem::Pr(pr) = item {
            let selected = idx == state.detail_selected;
            let icon = if pr.draft { "○" } else if pr.state == "closed" { "◆" } else { "●" };
            let icon_color = if pr.draft { Color::DarkGray } else if pr.state == "closed" { Color::Magenta } else { Color::Green };

            let style = if selected {
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            let age = pr.updated_at.map(|dt| {
                let ago = Utc::now().signed_duration_since(dt);
                if ago.num_hours() < 1 { format!("{}m ago", ago.num_minutes()) }
                else if ago.num_hours() < 24 { format!("{}h ago", ago.num_hours()) }
                else { format!("{}d ago", ago.num_days()) }
            }).unwrap_or_default();

            lines.push(Line::from(vec![
                Span::raw(if selected { "  > " } else { "    " }),
                Span::styled(icon, Style::default().fg(icon_color)),
                Span::styled(format!(" #{:<5}", pr.number), Style::default().fg(Color::DarkGray)),
                Span::styled(&pr.title, style),
                Span::raw("  "),
                Span::styled(&pr.user, Style::default().fg(Color::Magenta)),
            ]));
            lines.push(Line::from(vec![
                Span::raw("         "),
                Span::styled(format!("{} ← {}", pr.base_ref, pr.head_ref), Style::default().fg(Color::DarkGray)),
                Span::raw("  "),
                Span::styled(age, Style::default().fg(Color::DarkGray)),
            ]));
            lines.push(Line::from(""));
        }
    }

    // Scroll
    let item_height = 3;
    let selected_top = state.detail_selected * item_height;
    let visible = area.height as usize;
    let scroll = if selected_top + item_height > visible { selected_top + item_height - visible } else { 0 };

    let visible_lines: Vec<Line> = lines.into_iter().skip(scroll).take(visible).collect();
    frame.render_widget(Paragraph::new(visible_lines), area);
}

fn render_issue_list(frame: &mut Frame, area: Rect, state: &AppState) {
    let items = state.filtered_detail_items();
    if items.is_empty() {
        frame.render_widget(
            Paragraph::new("  No issues").style(Style::default().fg(Color::DarkGray)),
            area,
        );
        return;
    }

    let mut lines: Vec<Line> = Vec::new();
    for (idx, item) in items.iter().enumerate() {
        if let crate::app::DetailItem::Issue(issue) = item {
            let selected = idx == state.detail_selected;
            let icon = if issue.state == "closed" { "●" } else { "○" };
            let icon_color = if issue.state == "closed" { Color::Red } else { Color::Green };

            let style = if selected {
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            let labels_str = if issue.labels.is_empty() {
                String::new()
            } else {
                format!(" [{}]", issue.labels.join(", "))
            };

            let age = issue.updated_at.map(|dt| {
                let ago = Utc::now().signed_duration_since(dt);
                if ago.num_hours() < 24 { format!("{}h ago", ago.num_hours()) }
                else { format!("{}d ago", ago.num_days()) }
            }).unwrap_or_default();

            lines.push(Line::from(vec![
                Span::raw(if selected { "  > " } else { "    " }),
                Span::styled(icon, Style::default().fg(icon_color)),
                Span::styled(format!(" #{:<5}", issue.number), Style::default().fg(Color::DarkGray)),
                Span::styled(&issue.title, style),
                Span::styled(labels_str, Style::default().fg(Color::Yellow)),
            ]));
            lines.push(Line::from(vec![
                Span::raw("         "),
                Span::styled(&issue.user, Style::default().fg(Color::Magenta)),
                Span::raw("  "),
                Span::styled(age, Style::default().fg(Color::DarkGray)),
                Span::raw("  "),
                Span::styled(format!("💬 {}", issue.comments), Style::default().fg(Color::DarkGray)),
            ]));
            lines.push(Line::from(""));
        }
    }

    let item_height = 3;
    let selected_top = state.detail_selected * item_height;
    let visible = area.height as usize;
    let scroll = if selected_top + item_height > visible { selected_top + item_height - visible } else { 0 };
    let visible_lines: Vec<Line> = lines.into_iter().skip(scroll).take(visible).collect();
    frame.render_widget(Paragraph::new(visible_lines), area);
}

fn render_ci_list(frame: &mut Frame, area: Rect, state: &AppState) {
    let items = state.filtered_detail_items();
    if items.is_empty() {
        frame.render_widget(
            Paragraph::new("  No CI runs").style(Style::default().fg(Color::DarkGray)),
            area,
        );
        return;
    }

    let mut lines: Vec<Line> = Vec::new();
    for (idx, item) in items.iter().enumerate() {
        if let crate::app::DetailItem::Ci(run) = item {
            let selected = idx == state.detail_selected;
            let (icon, icon_color) = match (run.status.as_str(), run.conclusion.as_deref()) {
                ("completed", Some("success")) => ("✔", Color::Green),
                ("completed", Some("failure")) => ("✖", Color::Red),
                ("completed", Some("cancelled")) => ("–", Color::Yellow),
                ("in_progress", _) => ("◌", Color::Yellow),
                ("queued", _) => ("·", Color::DarkGray),
                _ => ("?", Color::DarkGray),
            };

            let style = if selected {
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            let duration = run.duration_secs.map(|s| {
                if s < 60 { format!("{}s", s) }
                else { format!("{}m{}s", s / 60, s % 60) }
            }).unwrap_or_default();

            let age = run.created_at.map(|dt| {
                let ago = Utc::now().signed_duration_since(dt);
                if ago.num_hours() < 1 { format!("{}m ago", ago.num_minutes()) }
                else if ago.num_hours() < 24 { format!("{}h ago", ago.num_hours()) }
                else { format!("{}d ago", ago.num_days()) }
            }).unwrap_or_default();

            lines.push(Line::from(vec![
                Span::raw(if selected { "  > " } else { "    " }),
                Span::styled(icon, Style::default().fg(icon_color)),
                Span::raw(" "),
                Span::styled(&run.name, style),
            ]));
            lines.push(Line::from(vec![
                Span::raw("       "),
                Span::styled(&run.head_branch, Style::default().fg(Color::Magenta)),
                Span::raw("  "),
                Span::styled(duration, Style::default().fg(Color::DarkGray)),
                Span::raw("  "),
                Span::styled(age, Style::default().fg(Color::DarkGray)),
            ]));
            lines.push(Line::from(""));
        }
    }

    let item_height = 3;
    let selected_top = state.detail_selected * item_height;
    let visible = area.height as usize;
    let scroll = if selected_top + item_height > visible { selected_top + item_height - visible } else { 0 };
    let visible_lines: Vec<Line> = lines.into_iter().skip(scroll).take(visible).collect();
    frame.render_widget(Paragraph::new(visible_lines), area);
}
