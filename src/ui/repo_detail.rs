use chrono::Utc;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::{AppState, DetailFocus, RepoSection};

pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
    let (repo_full_name, section) = match &state.screen {
        crate::app::Screen::RepoDetail { repo_full_name, section } => (repo_full_name, *section),
        _ => return,
    };

    let has_activity = !state.repo_commit_activity.is_empty();
    let heatmap_height = if has_activity { 6 } else { 0 };

    let [header_area, heatmap_area, tabs_area, content_area] = Layout::vertical([
        Constraint::Length(4),
        Constraint::Length(heatmap_height),
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

    // ── Repo commit heatmap (always visible) ──
    if has_activity {
        render_repo_heatmap(frame, heatmap_area, &state.repo_commit_activity);
    }

    // ── Section tabs ──
    let focused = state.detail_focus == DetailFocus::TabBar;
    let tabs: Vec<(RepoSection, String)> = vec![
        (RepoSection::PRs, format!("PRs ({})", state.repo_prs.len())),
        (RepoSection::Issues, format!("Issues ({})", state.repo_issues.len())),
        (RepoSection::CI, format!("CI ({})", state.repo_ci.len())),
        (RepoSection::Commits, format!("Commits ({})", state.repo_commits.len())),
        (RepoSection::Info, "Info".to_string()),
    ];

    let mut tab_spans: Vec<Span> = vec![Span::raw("  ")];
    let mut underline_spans: Vec<Span> = vec![Span::raw("  ")];
    for (i, (sec, label)) in tabs.iter().enumerate() {
        if i > 0 {
            tab_spans.push(Span::raw("  "));
            underline_spans.push(Span::raw("  "));
        }
        let active = section == *sec;
        let width = label.len();
        let style = if active && focused {
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
        } else if active {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        tab_spans.push(Span::styled(label.clone(), style));
        let bar: String = if active { "━".repeat(width) } else { "─".repeat(width) };
        let bar_style = if active { Style::default().fg(Color::Cyan) } else { Style::default().fg(Color::DarkGray) };
        underline_spans.push(Span::styled(bar, bar_style));
    }

    frame.render_widget(Paragraph::new(vec![Line::from(tab_spans), Line::from(underline_spans)]), tabs_area);

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
        RepoSection::Commits => render_commit_list(frame, content_area, state),
        RepoSection::Info => render_readme(frame, content_area, state),
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
            let icon = if pr.draft { "○" } else if pr.merged { "◆" } else if pr.state == "closed" { "●" } else { "●" };
            let icon_color = if pr.draft { Color::DarkGray } else if pr.merged { Color::Magenta } else if pr.state == "closed" { Color::Red } else { Color::Green };

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
            let diff_stats = if pr.additions > 0 || pr.deletions > 0 {
                format!("  +{} -{}", pr.additions, pr.deletions)
            } else {
                String::new()
            };
            lines.push(Line::from(vec![
                Span::raw("         "),
                Span::styled(format!("{} ← {}", pr.base_ref, pr.head_ref), Style::default().fg(Color::DarkGray)),
                Span::raw("  "),
                Span::styled(age, Style::default().fg(Color::DarkGray)),
                Span::styled(diff_stats, Style::default().fg(Color::Green)),
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
                if ago.num_hours() < 1 { format!("{}m ago", ago.num_minutes()) }
                else if ago.num_hours() < 24 { format!("{}h ago", ago.num_hours()) }
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

fn render_commit_list(frame: &mut Frame, area: Rect, state: &AppState) {
    let items = state.filtered_detail_items();
    if items.is_empty() {
        frame.render_widget(
            Paragraph::new("  No commits").style(Style::default().fg(Color::DarkGray)),
            area,
        );
        return;
    }

    let mut lines: Vec<Line> = Vec::new();
    for (idx, item) in items.iter().enumerate() {
        if let crate::app::DetailItem::Commit(commit) = item {
            let selected = idx == state.detail_selected;
            let is_merge = commit.parents.len() > 1;

            let style = if selected {
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            let graph_char = if is_merge { "*  " } else { "* " };
            let prefix = if selected { "  > " } else { "    " };

            lines.push(Line::from(vec![
                Span::raw(prefix),
                Span::styled(graph_char, Style::default().fg(Color::Green)),
                Span::styled(&commit.short_sha, Style::default().fg(Color::Yellow)),
                Span::raw(" "),
                Span::styled(&commit.message, style),
            ]));

            let age = {
                let ago = Utc::now().signed_duration_since(commit.date);
                if ago.num_hours() < 1 { format!("{}m ago", ago.num_minutes()) }
                else if ago.num_hours() < 24 { format!("{}h ago", ago.num_hours()) }
                else { format!("{}d ago", ago.num_days()) }
            };

            lines.push(Line::from(vec![
                Span::raw(if is_merge { "       |\\  " } else { "       |  " }),
                Span::styled(&commit.author, Style::default().fg(Color::Magenta)),
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

fn render_readme(frame: &mut Frame, area: Rect, state: &AppState) {
    let content = match &state.repo_readme {
        Some(c) => c,
        None => {
            frame.render_widget(
                Paragraph::new("  No README available").style(Style::default().fg(Color::DarkGray)),
                area,
            );
            return;
        }
    };

    let mut lines: Vec<Line> = Vec::new();
    let mut in_code_block = false;

    for raw_line in content.lines() {
        // Code block toggle
        if raw_line.trim_start().starts_with("```") {
            in_code_block = !in_code_block;
            lines.push(Line::from(Span::styled(
                format!("  {}", raw_line),
                Style::default().fg(Color::DarkGray),
            )));
            continue;
        }

        if in_code_block {
            lines.push(Line::from(Span::styled(
                format!("    {}", raw_line),
                Style::default().fg(Color::Green),
            )));
            continue;
        }

        let trimmed = raw_line.trim();

        // Headers
        if let Some(h) = trimmed.strip_prefix("### ") {
            lines.push(Line::from(Span::styled(
                format!("  {}", h),
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
            )));
        } else if let Some(h) = trimmed.strip_prefix("## ") {
            lines.push(Line::from(Span::styled(
                format!("  {}", h),
                Style::default().fg(Color::Cyan),
            )));
        } else if let Some(h) = trimmed.strip_prefix("# ") {
            lines.push(Line::from(Span::styled(
                format!("  {}", h),
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            )));
        }
        // List items
        else if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled("• ", Style::default().fg(Color::Cyan)),
                Span::raw(trimmed[2..].to_string()),
            ]));
        }
        // Blank lines
        else if trimmed.is_empty() {
            lines.push(Line::from(""));
        }
        // Regular text — parse inline formatting
        else {
            lines.push(parse_inline_markdown(trimmed));
        }
    }

    // Scroll based on detail_selected (repurpose as scroll position for Info tab)
    let scroll = state.detail_selected;
    let visible = area.height as usize;
    let visible_lines: Vec<Line> = lines.into_iter().skip(scroll).take(visible).collect();
    frame.render_widget(Paragraph::new(visible_lines), area);
}

/// Parse inline markdown: **bold**, *italic*, `code`, [links](url)
fn parse_inline_markdown(text: &str) -> Line<'static> {
    let mut spans: Vec<Span> = vec![Span::raw("  ".to_string())]; // left padding
    let mut chars = text.chars().peekable();
    let mut buf = String::new();

    while let Some(ch) = chars.next() {
        match ch {
            '`' => {
                if !buf.is_empty() {
                    spans.push(Span::raw(buf.clone()));
                    buf.clear();
                }
                let mut code = String::new();
                while let Some(&c) = chars.peek() {
                    if c == '`' { chars.next(); break; }
                    code.push(c);
                    chars.next();
                }
                spans.push(Span::styled(code, Style::default().fg(Color::Yellow)));
            }
            '*' if chars.peek() == Some(&'*') => {
                chars.next(); // consume second *
                if !buf.is_empty() {
                    spans.push(Span::raw(buf.clone()));
                    buf.clear();
                }
                let mut bold = String::new();
                while let Some(c) = chars.next() {
                    if c == '*' && chars.peek() == Some(&'*') { chars.next(); break; }
                    bold.push(c);
                }
                spans.push(Span::styled(bold, Style::default().add_modifier(Modifier::BOLD)));
            }
            '*' => {
                if !buf.is_empty() {
                    spans.push(Span::raw(buf.clone()));
                    buf.clear();
                }
                let mut italic = String::new();
                for c in chars.by_ref() {
                    if c == '*' { break; }
                    italic.push(c);
                }
                spans.push(Span::styled(italic, Style::default().add_modifier(Modifier::ITALIC)));
            }
            '[' => {
                if !buf.is_empty() {
                    spans.push(Span::raw(buf.clone()));
                    buf.clear();
                }
                let mut link_text = String::new();
                for c in chars.by_ref() {
                    if c == ']' { break; }
                    link_text.push(c);
                }
                // Skip the (url) part
                if chars.peek() == Some(&'(') {
                    chars.next();
                    for c in chars.by_ref() {
                        if c == ')' { break; }
                    }
                }
                spans.push(Span::styled(
                    link_text,
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::UNDERLINED),
                ));
            }
            _ => buf.push(ch),
        }
    }
    if !buf.is_empty() {
        spans.push(Span::raw(buf));
    }
    Line::from(spans)
}

fn render_repo_heatmap(
    frame: &mut Frame,
    area: Rect,
    activity: &[crate::github::commits::WeeklyCommitActivity],
) {
    if activity.is_empty() { return; }

    // Find max daily count for level scaling
    let max_daily = activity.iter()
        .flat_map(|w| w.days.iter())
        .copied()
        .max()
        .unwrap_or(1)
        .max(1);

    // Limit weeks to fit width
    let label_width = 5usize; // "Mon " etc
    let max_weeks = (area.width as usize).saturating_sub(label_width);
    let visible = if activity.len() > max_weeks {
        &activity[activity.len() - max_weeks..]
    } else {
        activity
    };

    let mut lines: Vec<Line> = Vec::new();

    // Day rows: Mon, Wed, Fri (compact — skip Tue, Thu, Sat, Sun)
    // GitHub stats API returns days as [Sun, Mon, Tue, Wed, Thu, Fri, Sat]
    let day_rows = [(1, "Mon"), (3, "Wed"), (5, "Fri")];
    for (day_idx, day_label) in day_rows {
        let mut spans: Vec<Span> = vec![Span::styled(
            format!(" {} ", day_label),
            Style::default().fg(Color::DarkGray),
        )];
        for week in visible {
            let count = week.days[day_idx];
            let level = if count == 0 { 0 }
                else if count <= max_daily / 4 { 1 }
                else if count <= max_daily / 2 { 2 }
                else if count <= max_daily * 3 / 4 { 3 }
                else { 4 };
            let (ch, style) = level_to_cell(level);
            spans.push(Span::styled(String::from(ch), style));
        }
        lines.push(Line::from(spans));
    }

    // Stats line
    let total: u32 = activity.iter().map(|w| w.total).sum();
    lines.push(Line::from(Span::styled(
        format!("      {} commits this year", total),
        Style::default().fg(Color::DarkGray),
    )));

    frame.render_widget(Paragraph::new(lines), area);
}

fn level_to_cell(level: u8) -> (char, Style) {
    match level {
        0 => ('\u{2591}', Style::default().fg(Color::DarkGray)),
        1 => ('\u{2592}', Style::default().fg(Color::Green)),
        2 => ('\u{2593}', Style::default().fg(Color::Green)),
        3 => ('\u{2588}', Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
        4 => ('\u{2588}', Style::default().fg(Color::LightGreen).add_modifier(Modifier::BOLD)),
        _ => ('\u{2591}', Style::default().fg(Color::DarkGray)),
    }
}
