use chrono::{Datelike, Utc};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::{AppState, ViewMode};
use crate::github::avatar;
use crate::github::contributions::ContributionDay;
use crate::github::repos::RepoInfo;

pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
    // Split: profile+graph panel on top, card grid below
    let show_avatar = state.term_width >= 80;
    let profile_height: u16 = if show_avatar { 10 } else { 9 }; // graph only when narrow

    let [top_area, cards_area] = Layout::vertical([
        Constraint::Length(profile_height),
        Constraint::Fill(1),
    ]).areas(area);

    if show_avatar {
        render_profile_and_graph(frame, top_area, state);
    } else {
        // Just render the heatmap without the avatar/profile panel
        render_heatmap(frame, top_area, &state.contributions.days, state.contributions.total);
    }

    let filtered = state.filtered_repos();
    match state.view_mode {
        ViewMode::Cards => render_card_grid(frame, cards_area, &filtered, state.card_selected, state.num_card_cols()),
        ViewMode::List => render_list_view(frame, cards_area, &filtered, state.card_selected),
    }
}

fn render_profile_and_graph(frame: &mut Frame, area: Rect, state: &AppState) {
    // Side by side: avatar+info (fixed width) | contribution graph (fill)
    let avatar_width = 16u16;

    let [profile_area, graph_area] = Layout::horizontal([
        Constraint::Length(avatar_width),
        Constraint::Fill(1),
    ]).areas(area);

    // ── Profile panel ──
    let mut profile_lines: Vec<Line> = Vec::new();

    // Render avatar if available
    if let Some(ref img) = state.avatar {
        let art_w = profile_area.width.saturating_sub(2); // padding
        let art_h = profile_area.height.saturating_sub(4); // leave room for text
        let avatar_lines = avatar::image_to_halfblocks(img, art_w, art_h);
        profile_lines.extend(avatar_lines);
    }

    // User info
    if let Some(ref info) = state.user_info {
        profile_lines.push(Line::from(Span::styled(
            &info.login,
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        )));
        let total_stars: u32 = state.repos.iter().map(|r| r.stargazers_count).sum();
        profile_lines.push(Line::from(Span::styled(
            format!("{} repos · {} orgs", info.public_repos, count_orgs(&state.repos)),
            Style::default().fg(Color::DarkGray),
        )));
        profile_lines.push(Line::from(Span::styled(
            format!("★ {} · {} followers", total_stars, info.followers),
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        profile_lines.push(Line::from(Span::styled("Loading...", Style::default().fg(Color::DarkGray))));
    }

    frame.render_widget(Paragraph::new(profile_lines), profile_area);

    // ── Contribution graph ──
    render_heatmap(frame, graph_area, &state.contributions.days, state.contributions.total);
}

fn render_heatmap(frame: &mut Frame, area: Rect, days: &[ContributionDay], total: u32) {
    if days.is_empty() {
        let p = Paragraph::new("Loading contributions...")
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(p, area);
        return;
    }

    // Group days by week
    let mut weeks: Vec<[Option<&ContributionDay>; 7]> = Vec::new();
    let mut current_week: [Option<&ContributionDay>; 7] = [None; 7];

    for day in days {
        let weekday = day.date.weekday().num_days_from_monday() as usize;
        if weekday == 0 && current_week.iter().any(|d| d.is_some()) {
            weeks.push(current_week);
            current_week = [None; 7];
        }
        current_week[weekday] = Some(day);
    }
    if current_week.iter().any(|d| d.is_some()) {
        weeks.push(current_week);
    }

    // Limit weeks to fit available width
    let label_width = 5; // "Mon "
    let max_weeks = (area.width.saturating_sub(label_width)) as usize;
    let visible_weeks = if weeks.len() > max_weeks {
        &weeks[weeks.len() - max_weeks..]
    } else {
        &weeks
    };

    // Month labels
    let mut month_spans: Vec<Span> = vec![Span::raw("     ")];
    let mut last_month = None;
    for week in visible_weeks {
        let month = week.iter().flatten().next().map(|d| d.date.format("%b").to_string());
        if month != last_month {
            if let Some(ref m) = month {
                month_spans.push(Span::styled(m.clone(), Style::default().fg(Color::DarkGray)));
                last_month = month;
            } else {
                month_spans.push(Span::raw(" "));
            }
        } else {
            month_spans.push(Span::raw(" "));
        }
    }

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(month_spans));

    let day_labels = ["Mon", "   ", "Wed", "   ", "Fri", "   ", "Sun"];
    for (row_idx, day_label) in day_labels.iter().enumerate() {
        let mut spans: Vec<Span> = vec![Span::styled(
            format!(" {} ", day_label),
            Style::default().fg(Color::DarkGray),
        )];
        for week in visible_weeks {
            let cell = week[row_idx];
            let (ch, style) = match cell {
                Some(day) => level_to_cell(day.level),
                None => (' ', Style::default()),
            };
            spans.push(Span::styled(String::from(ch), style));
        }
        lines.push(Line::from(spans));
    }

    // Stats line
    let (current_streak, _longest_streak) = calculate_streaks(days);
    let streak_text = if current_streak > 0 {
        format!("  {} contributions · {} day streak 🔥", format_with_commas(total), current_streak)
    } else {
        format!("  {} contributions", format_with_commas(total))
    };
    lines.push(Line::from(Span::styled(streak_text, Style::default().fg(Color::DarkGray))));

    frame.render_widget(Paragraph::new(lines), area);
}

fn render_card_grid(frame: &mut Frame, area: Rect, repos: &[&RepoInfo], selected: usize, num_cols: usize) {
    if repos.is_empty() {
        let p = Paragraph::new("  No repos found")
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(p, area);
        return;
    }

    // Group repos by owner
    let mut groups: Vec<(&str, Vec<(usize, &&RepoInfo)>)> = Vec::new();
    let mut current_owner: Option<&str> = None;

    for (idx, repo) in repos.iter().enumerate() {
        let owner = repo.owner.as_str();
        if current_owner != Some(owner) {
            groups.push((owner, Vec::new()));
            current_owner = Some(owner);
        }
        groups.last_mut().unwrap().1.push((idx, repo));
    }

    let card_height = 7u16; // 5 content + 2 border
    let mut lines_rendered = 0u16;

    // Calculate which row the selected card is in (for scrolling)
    let selected_visual_row = find_visual_row(selected, &groups, num_cols);
    let visible_rows = area.height / card_height;
    let scroll_row = if selected_visual_row >= visible_rows as usize {
        selected_visual_row - visible_rows as usize + 1
    } else {
        0
    };

    let mut visual_row = 0usize;

    for (org, org_repos) in &groups {
        // Org header
        if visual_row >= scroll_row {
            let header_y = area.y + lines_rendered;
            if lines_rendered + 2 > area.height { break; }

            let mode_indicator = format!("[v/l]");
            let header_line = Line::from(vec![
                Span::styled(
                    format!("  {} ({})  ", org, org_repos.len()),
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                ),
                Span::styled(mode_indicator, Style::default().fg(Color::DarkGray)),
            ]);
            frame.render_widget(
                Paragraph::new(header_line),
                Rect { x: area.x, y: header_y, width: area.width, height: 1 },
            );
            lines_rendered += 2; // header + gap
        } else {
            // Skip this header
        }
        visual_row += 1; // header counts as a visual row for scrolling purposes (not exactly a card row but close enough)

        // Card rows
        for chunk in org_repos.chunks(num_cols) {
            if visual_row >= scroll_row {
                if lines_rendered + card_height > area.height { break; }

                let row_y = area.y + lines_rendered;
                let card_width = area.width / num_cols as u16;

                for (col_idx, (flat_idx, repo)) in chunk.iter().enumerate() {
                    let card_x = area.x + (col_idx as u16 * card_width);
                    let w = if col_idx == num_cols - 1 {
                        area.width - (col_idx as u16 * card_width) // last card takes remaining
                    } else {
                        card_width
                    };
                    let card_area = Rect { x: card_x, y: row_y, width: w, height: card_height };
                    render_card(frame, card_area, repo, *flat_idx == selected);
                }
                lines_rendered += card_height;
            }
            visual_row += 1;
        }
    }
}

fn render_card(frame: &mut Frame, area: Rect, repo: &RepoInfo, selected: bool) {
    let border_style = if selected {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height == 0 || inner.width == 0 { return; }

    let name = repo.full_name.split('/').last().unwrap_or(&repo.full_name);
    let visibility = if repo.is_private { "🔒 " } else { "   " };
    let stars = if repo.stargazers_count > 0 {
        format!("★ {}", repo.stargazers_count)
    } else {
        String::new()
    };

    let desc = repo.description.as_deref().unwrap_or("");
    let max_desc = (inner.width as usize).saturating_sub(2);
    let desc_truncated = if desc.len() > max_desc {
        format!("{}…", &desc[..max_desc.saturating_sub(1)])
    } else {
        desc.to_string()
    };

    let lang = repo.language.as_deref().unwrap_or("");
    let forks = if repo.forks_count > 0 { format!("⎚{}", repo.forks_count) } else { String::new() };

    let pushed = repo.pushed_at.map(|dt| {
        let ago = Utc::now().signed_duration_since(dt);
        if ago.num_hours() < 1 { format!("{}m ago", ago.num_minutes()) }
        else if ago.num_hours() < 24 { format!("{}h ago", ago.num_hours()) }
        else if ago.num_days() < 30 { format!("{}d ago", ago.num_days()) }
        else { format!("{}mo ago", ago.num_days() / 30) }
    }).unwrap_or_default();

    let mut lines = vec![
        // Line 1: name + stars
        Line::from(vec![
            Span::raw(visibility),
            Span::styled(name, Style::default().add_modifier(Modifier::BOLD)),
            Span::raw("  "),
            Span::styled(&stars, Style::default().fg(Color::Yellow)),
        ]),
        // Line 2: description
        Line::from(Span::styled(
            format!(" {}", desc_truncated),
            Style::default().fg(Color::DarkGray),
        )),
        // Line 3: blank
        Line::from(""),
        // Line 4: language + forks
        Line::from(vec![
            Span::styled(format!(" {}", lang), Style::default().fg(Color::Magenta)),
            Span::raw("  "),
            Span::styled(&forks, Style::default().fg(Color::DarkGray)),
        ]),
        // Line 5: last push time
        Line::from(Span::styled(
            format!(" {}", pushed),
            Style::default().fg(Color::DarkGray),
        )),
    ];

    // Trim to available height
    lines.truncate(inner.height as usize);

    frame.render_widget(Paragraph::new(lines), inner);
}

fn render_list_view(frame: &mut Frame, area: Rect, repos: &[&RepoInfo], selected: usize) {
    if repos.is_empty() {
        let p = Paragraph::new("  No repos found")
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(p, area);
        return;
    }

    // Group by owner
    let mut groups: Vec<(&str, Vec<(usize, &&RepoInfo)>)> = Vec::new();
    let mut current_owner: Option<&str> = None;
    for (idx, repo) in repos.iter().enumerate() {
        let owner = repo.owner.as_str();
        if current_owner != Some(owner) {
            groups.push((owner, Vec::new()));
            current_owner = Some(owner);
        }
        groups.last_mut().unwrap().1.push((idx, repo));
    }

    let mut lines: Vec<Line> = Vec::new();

    for (org, org_repos) in &groups {
        lines.push(Line::from(Span::styled(
            format!("  {} ({})", org, org_repos.len()),
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        )));

        for (flat_idx, repo) in org_repos {
            let prefix = if *flat_idx == selected { " > " } else { "   " };
            let visibility = if repo.is_private { "🔒" } else { "  " };
            let name = repo.full_name.split('/').last().unwrap_or(&repo.full_name);
            let lang = repo.language.as_deref().unwrap_or("");
            let stars = if repo.stargazers_count > 0 {
                format!("★{}", repo.stargazers_count)
            } else {
                String::new()
            };
            let pushed = repo.pushed_at.map(|dt| {
                let ago = Utc::now().signed_duration_since(dt);
                if ago.num_hours() < 1 { format!("{}m", ago.num_minutes()) }
                else if ago.num_hours() < 24 { format!("{}h", ago.num_hours()) }
                else { format!("{}d", ago.num_days()) }
            }).unwrap_or_default();

            let style = if *flat_idx == selected {
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            let forks = if repo.forks_count > 0 {
                format!("⎚{}", repo.forks_count)
            } else {
                String::new()
            };

            lines.push(Line::from(vec![
                Span::styled(prefix, style),
                Span::raw(visibility),
                Span::styled(format!(" {:<20}", name), style),
                Span::styled(format!("{:<8}", lang), Style::default().fg(Color::Magenta)),
                Span::styled(format!("{:<6}", stars), Style::default().fg(Color::Yellow)),
                Span::styled(format!("{:<5}", forks), Style::default().fg(Color::DarkGray)),
                Span::styled(pushed, Style::default().fg(Color::DarkGray)),
            ]));
        }
        lines.push(Line::from("")); // gap between groups
    }

    // Scroll: ensure selected item is visible
    let selected_line = lines.iter().position(|l| {
        l.spans.first().is_some_and(|s| s.content.contains('>'))
    }).unwrap_or(0);
    let scroll = if selected_line >= area.height as usize {
        selected_line - area.height as usize + 2
    } else {
        0
    };

    let visible: Vec<Line> = lines.into_iter().skip(scroll).take(area.height as usize).collect();
    frame.render_widget(Paragraph::new(visible), area);
}

fn find_visual_row(selected: usize, groups: &[(&str, Vec<(usize, &&RepoInfo)>)], num_cols: usize) -> usize {
    let mut row = 0;
    for (_org, org_repos) in groups {
        row += 1; // org header
        for chunk in org_repos.chunks(num_cols) {
            if chunk.iter().any(|(idx, _)| *idx == selected) {
                return row;
            }
            row += 1;
        }
    }
    row
}

fn count_orgs(repos: &[RepoInfo]) -> usize {
    let mut owners: Vec<&str> = repos.iter().map(|r| r.owner.as_str()).collect();
    owners.sort_unstable();
    owners.dedup();
    owners.len()
}

// ── Ported from ui/contributions.rs ──

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

fn calculate_streaks(days: &[ContributionDay]) -> (u32, u32) {
    let today = Utc::now().date_naive();
    let mut current = 0u32;
    for day in days.iter().rev() {
        if day.date > today { continue; }
        if day.count > 0 {
            current += 1;
        } else if day.date == today {
            continue;
        } else {
            break;
        }
    }
    let mut longest = 0u32;
    let mut run = 0u32;
    for day in days {
        if day.count > 0 {
            run += 1;
            longest = longest.max(run);
        } else {
            run = 0;
        }
    }
    (current, longest)
}

fn format_with_commas(n: u32) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, ch) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 { result.push(','); }
        result.push(ch);
    }
    result.chars().rev().collect()
}
