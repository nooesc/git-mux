use chrono::{Datelike, Utc};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use std::sync::{Mutex, OnceLock};

use crate::app::{AppState, HomeFocus, RepoFilter, ViewMode};
use crate::github::avatar;
use crate::github::contributions::ContributionDay;
use crate::github::repos::RepoInfo;

pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
    // Split: profile+graph panel on top, card grid below
    let show_avatar = state.term_width >= 80;
    let profile_height: u16 = if show_avatar { 12 } else { 10 };

    let [top_area, filter_area, cards_area] = Layout::vertical([
        Constraint::Length(profile_height),
        Constraint::Length(3),
        Constraint::Fill(1),
    ])
    .areas(area);

    if show_avatar {
        render_profile_and_graph(frame, top_area, state);
    } else {
        // Just render the heatmap without the avatar/profile panel
        render_heatmap(
            frame,
            top_area,
            &state.contributions.days,
            state.contributions.total,
        );
    }

    render_filter_bar(frame, filter_area, state);

    let filtered = state.filtered_repos();
    let group = state.repo_filter != RepoFilter::All;
    match state.view_mode {
        ViewMode::Cards => render_card_grid(
            frame,
            cards_area,
            &filtered,
            state.card_selected,
            state.num_card_cols(),
            group,
        ),
        ViewMode::List => {
            render_list_view(frame, cards_area, &filtered, state.card_selected, group)
        }
    }
}

fn render_profile_and_graph(frame: &mut Frame, area: Rect, state: &AppState) {
    // Three columns: info | avatar | contribution graph
    let avatar_width = 30u16;
    let info_width = 20u16;

    let [info_area, avatar_area, graph_area] = Layout::horizontal([
        Constraint::Length(info_width),
        Constraint::Length(avatar_width),
        Constraint::Fill(1),
    ])
    .areas(area);

    // ── Avatar (centered) ──
    let avatar_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(Span::styled(" Avatar ", Style::default().fg(Color::Cyan)));
    let avatar_inner = avatar_block.inner(avatar_area);
    frame.render_widget(avatar_block, avatar_area);

    let avatar_lines = if let Some(ref img) = state.avatar {
        centered_avatar_lines(img, avatar_inner.width, avatar_inner.height)
    } else {
        vec![Line::from("")]
    };
    frame.render_widget(Paragraph::new(avatar_lines), avatar_inner);

    // ── User info (compact list) ──
    let info_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(Span::styled(" Profile ", Style::default().fg(Color::Cyan)));
    let info_inner = info_block.inner(info_area);
    frame.render_widget(info_block, info_area);

    let mut info_lines: Vec<Line> = Vec::new();
    info_lines.push(Line::from(""));
    if let Some(ref info) = state.user_info {
        let w = info_inner.width as usize;
        // username (+ real name if available)
        if let Some(ref name) = info.name {
            info_lines.push(Line::from(vec![
                Span::styled(
                    &info.login,
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(" | ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    truncate_ellipsis(name, w.saturating_sub(info.login.len() + 3)),
                    Style::default().fg(Color::DarkGray),
                ),
            ]));
        } else {
            info_lines.push(Line::from(Span::styled(
                &info.login,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )));
        }

        // bio
        if let Some(ref bio) = info.bio {
            info_lines.push(Line::from(Span::styled(
                truncate_ellipsis(bio, w),
                Style::default().fg(Color::White),
            )));
        }

        // location / company
        let mut meta_parts: Vec<String> = Vec::new();
        if let Some(ref loc) = info.location {
            meta_parts.push(loc.clone());
        }
        if let Some(ref company) = info.company {
            meta_parts.push(company.clone());
        }
        if !meta_parts.is_empty() {
            info_lines.push(Line::from(Span::styled(
                truncate_ellipsis(&meta_parts.join(" | "), w),
                Style::default().fg(Color::DarkGray),
            )));
        }

        // spacer between info and stats
        info_lines.push(Line::from(""));
        info_lines.push(Line::from(""));

        // stats
        let total_stars: u32 = state.repos.iter().map(|r| r.stargazers_count).sum();
        let orgs = count_orgs(&state.repos);
        info_lines.push(Line::from(vec![
            Span::styled(
                format!("★ {}", total_stars),
                Style::default().fg(Color::Yellow),
            ),
            Span::styled(" | ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{} followers", info.followers),
                Style::default().fg(Color::DarkGray),
            ),
        ]));
        info_lines.push(Line::from(Span::styled(
            format!("{} repos | {} orgs", info.public_repos, orgs),
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        info_lines.push(Line::from(Span::styled(
            "Loading...",
            Style::default().fg(Color::DarkGray),
        )));
    }
    info_lines.truncate(info_inner.height as usize);
    frame.render_widget(Paragraph::new(info_lines), info_inner);

    // ── Contribution graph ──
    let graph_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(Span::styled(
            " Contribution Chart ",
            Style::default().fg(Color::Cyan),
        ));
    let graph_inner = graph_block.inner(graph_area);
    frame.render_widget(graph_block, graph_area);

    render_heatmap(
        frame,
        graph_inner,
        &state.contributions.days,
        state.contributions.total,
    );
}

#[derive(Clone)]
struct AvatarRenderCache {
    src_width: u32,
    src_height: u32,
    width: u16,
    height: u16,
    lines: Vec<Line<'static>>,
}

static AVATAR_RENDER_CACHE: OnceLock<Mutex<Option<AvatarRenderCache>>> = OnceLock::new();

fn cached_avatar_halfblocks(
    img: &image::DynamicImage,
    width: u16,
    height: u16,
) -> Vec<Line<'static>> {
    let cache = AVATAR_RENDER_CACHE.get_or_init(|| Mutex::new(None));
    let src_width = img.width();
    let src_height = img.height();

    if let Ok(mut guard) = cache.lock() {
        if let Some(cached) = guard.as_ref()
            && cached.src_width == src_width
            && cached.src_height == src_height
            && cached.width == width
            && cached.height == height
        {
            return cached.lines.clone();
        }

        let lines = avatar::image_to_halfblocks(img, width, height);
        *guard = Some(AvatarRenderCache {
            src_width,
            src_height,
            width,
            height,
            lines: lines.clone(),
        });
        return lines;
    }

    avatar::image_to_halfblocks(img, width, height)
}

fn centered_avatar_lines(
    img: &image::DynamicImage,
    container_w: u16,
    container_h: u16,
) -> Vec<Line<'static>> {
    if container_w == 0 || container_h == 0 {
        return Vec::new();
    }

    let blocks = cached_avatar_halfblocks(img, container_w, container_h);

    let mut out: Vec<Line<'static>> = Vec::new();
    for row in blocks {
        out.push(row);
    }

    while out.len() < container_h as usize {
        out.push(Line::from(" ".repeat(container_w as usize)));
    }

    out
}

fn render_filter_bar(frame: &mut Frame, area: Rect, state: &AppState) {
    let opts = state.filter_options();
    let focused = state.home_focus == HomeFocus::FilterBar;

    let mut spans: Vec<Span> = vec![Span::raw("  ")];

    for (i, opt) in opts.iter().enumerate() {
        let label = match opt {
            RepoFilter::All => "All".to_string(),
            RepoFilter::Public => "Public".to_string(),
            RepoFilter::Private => "Private".to_string(),
            RepoFilter::Org(name) => name.clone(),
        };

        let is_active = *opt == state.repo_filter;
        let is_cursor = focused && i == state.filter_index;

        let style = if is_active && is_cursor {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
        } else if is_active {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else if is_cursor {
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::UNDERLINED)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        spans.push(Span::styled(format!(" {} ", label), style));

        if i + 1 < opts.len() {
            spans.push(Span::styled("│", Style::default().fg(Color::DarkGray)));
        }
    }

    let border_color = if focused {
        Color::Cyan
    } else {
        Color::DarkGray
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));
    frame.render_widget(Paragraph::new(Line::from(spans)).block(block), area);
}

fn render_heatmap(frame: &mut Frame, area: Rect, days: &[ContributionDay], total: u32) {
    if days.is_empty() {
        let p =
            Paragraph::new("Loading contributions...").style(Style::default().fg(Color::DarkGray));
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

    // Month labels (fixed-width buffer prevents overflow)
    let chart_width = label_width as usize + visible_weeks.len();
    let mut month_buf: Vec<char> = vec![' '; chart_width];
    let mut last_month: Option<String> = None;
    let mut last_label_end: usize = 0;

    for (i, week) in visible_weeks.iter().enumerate() {
        let col = label_width as usize + i;
        let month = week
            .iter()
            .flatten()
            .next()
            .map(|d| d.date.format("%b").to_string());
        if month != last_month {
            if let Some(ref m) = month {
                if col >= last_label_end + 1 {
                    for (j, ch) in m.chars().enumerate() {
                        if col + j < chart_width {
                            month_buf[col + j] = ch;
                        }
                    }
                    last_label_end = col + m.len();
                }
                last_month = month;
            }
        }
    }

    let month_str: String = month_buf.into_iter().collect();
    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from("")); // top padding
    lines.push(Line::from(Span::styled(
        month_str,
        Style::default().fg(Color::DarkGray),
    )));

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
        format!(
            "  {} contributions · {} day streak 🔥",
            format_with_commas(total),
            current_streak
        )
    } else {
        format!("  {} contributions", format_with_commas(total))
    };
    lines.push(Line::from(Span::styled(
        streak_text,
        Style::default().fg(Color::DarkGray),
    )));

    frame.render_widget(Paragraph::new(lines), area);
}

fn render_card_grid(
    frame: &mut Frame,
    area: Rect,
    repos: &[&RepoInfo],
    selected: usize,
    num_cols: usize,
    group: bool,
) {
    if repos.is_empty() {
        let p = Paragraph::new("  No repos found").style(Style::default().fg(Color::DarkGray));
        frame.render_widget(p, area);
        return;
    }

    // Group repos by owner
    let mut groups: Vec<(&str, Vec<(usize, &&RepoInfo)>)> = Vec::new();
    if group {
        let mut current_owner: Option<&str> = None;
        for (idx, repo) in repos.iter().enumerate() {
            let owner = repo.owner.as_str();
            if current_owner != Some(owner) {
                groups.push((owner, Vec::new()));
                current_owner = Some(owner);
            }
            groups.last_mut().unwrap().1.push((idx, repo));
        }
    } else {
        // Flat list — single group with no header
        groups.push(("", repos.iter().enumerate().collect()));
    }

    let card_height = 7u16; // 5 content + 2 border
    let mut lines_rendered = 0u16;

    // Calculate which row the selected card is in (for scrolling)
    let selected_visual_row = find_visual_row(selected, &groups, num_cols, group);
    let visible_rows = area.height / card_height;
    let scroll_row = if selected_visual_row >= visible_rows as usize {
        selected_visual_row - visible_rows as usize + 1
    } else {
        0
    };

    let mut visual_row = 0usize;

    for (org, org_repos) in &groups {
        // Org header (only when grouping)
        if group {
            if visual_row >= scroll_row {
                let header_y = area.y + lines_rendered;
                if lines_rendered + 2 > area.height {
                    break;
                }

                let mode_indicator = format!("[v/l]");
                let header_line = Line::from(vec![
                    Span::styled(
                        format!("  {} ({})  ", org, org_repos.len()),
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(mode_indicator, Style::default().fg(Color::DarkGray)),
                ]);
                frame.render_widget(
                    Paragraph::new(header_line),
                    Rect {
                        x: area.x,
                        y: header_y,
                        width: area.width,
                        height: 1,
                    },
                );
                lines_rendered += 2; // header + gap
            }
            visual_row += 1;
        }

        // Card rows
        for chunk in org_repos.chunks(num_cols) {
            if visual_row >= scroll_row {
                if lines_rendered + card_height > area.height {
                    break;
                }

                let row_y = area.y + lines_rendered;
                let card_width = area.width / num_cols as u16;

                for (col_idx, (flat_idx, repo)) in chunk.iter().enumerate() {
                    let card_x = area.x + (col_idx as u16 * card_width);
                    let w = if col_idx == num_cols - 1 {
                        area.width - (col_idx as u16 * card_width) // last card takes remaining
                    } else {
                        card_width
                    };
                    let card_area = Rect {
                        x: card_x,
                        y: row_y,
                        width: w,
                        height: card_height,
                    };
                    render_card(frame, card_area, repo, *flat_idx == selected);
                }
                lines_rendered += card_height;
            }
            visual_row += 1;
        }
    }
}

fn render_card(frame: &mut Frame, area: Rect, repo: &RepoInfo, selected: bool) {
    let selected_green_bg = Color::Rgb(98, 114, 98);
    let (border_style, bg) = if selected {
        (Style::default().fg(Color::Green), selected_green_bg)
    } else {
        (Style::default().fg(Color::DarkGray), Color::Reset)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .style(Style::default().bg(bg));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height == 0 || inner.width == 0 {
        return;
    }

    let raw_name = repo.full_name.split('/').last().unwrap_or(&repo.full_name);
    let visibility = if repo.is_private { "🔒 " } else { "   " };
    let stars = format!("★ {}", compact_u32(repo.stargazers_count));
    let forks = format!("⑂ {}", compact_u32(repo.forks_count));
    let issues = repo
        .open_issues_only_count
        .unwrap_or(repo.open_issues_count);
    let prs = repo
        .open_prs_count
        .map(compact_u32)
        .unwrap_or_else(|| "-".to_string());

    let desc = repo.description.as_deref().unwrap_or("");
    let max_desc = (inner.width as usize).saturating_sub(2);

    let pushed = repo
        .pushed_at
        .map(|dt| {
            let ago = Utc::now().signed_duration_since(dt);
            if ago.num_hours() < 1 {
                format!("{}m ago", ago.num_minutes())
            } else if ago.num_hours() < 24 {
                format!("{}h ago", ago.num_hours())
            } else if ago.num_days() < 30 {
                format!("{}d ago", ago.num_days())
            } else {
                format!("{}mo ago", ago.num_days() / 30)
            }
        })
        .unwrap_or_default();

    let updated = pushed.clone();
    let title_left = format!("{}{}", visibility, raw_name);
    let line_w = inner.width as usize;
    let issue_text = format!("! {}", compact_u32(issues));
    let pr_text = format!("PR {}", prs);
    let stats_text = format!("{}  {}  {}  {}", stars, forks, issue_text, pr_text);
    let stats_w = display_width(&stats_text);
    let name_max_w = line_w.saturating_sub(stats_w);
    let name_text = truncate_ellipsis_chars(&title_left, name_max_w);
    let pad_w = line_w
        .saturating_sub(display_width(&name_text))
        .saturating_sub(stats_w);
    let (desc_line1, desc_line2) = split_desc_two_lines(desc, max_desc);
    let updated_line = if updated.is_empty() {
        String::new()
    } else if line_w > updated.len() {
        format!("{:>w$}", updated, w = line_w)
    } else {
        truncate_ellipsis(&updated, line_w)
    };

    let mut lines = vec![
        // Line 1: name + stats
        Line::from(vec![
            Span::styled(name_text, Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" ".repeat(pad_w)),
            Span::styled(stars, Style::default().fg(Color::Yellow)),
            Span::styled("  ", Style::default().fg(Color::DarkGray)),
            Span::styled(forks, Style::default().fg(Color::DarkGray)),
            Span::styled("  ", Style::default().fg(Color::DarkGray)),
            Span::styled(issue_text, Style::default().fg(Color::Red)),
            Span::styled("  ", Style::default().fg(Color::DarkGray)),
            Span::styled(pr_text, Style::default().fg(Color::Green)),
        ]),
        // Line 2-3: description (two lines)
        Line::from(Span::styled(
            format!(" {}", desc_line1),
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(Span::styled(
            format!(" {}", desc_line2),
            Style::default().fg(Color::DarkGray),
        )),
        // Line 4: spacer
        Line::from(""),
        // Line 5: updated time at bottom-right
        Line::from(Span::styled(
            updated_line,
            Style::default().fg(Color::DarkGray),
        )),
    ];

    // Trim to available height
    lines.truncate(inner.height as usize);

    frame.render_widget(Paragraph::new(lines), inner);
}

fn render_list_view(
    frame: &mut Frame,
    area: Rect,
    repos: &[&RepoInfo],
    selected: usize,
    group: bool,
) {
    if repos.is_empty() {
        let p = Paragraph::new("  No repos found").style(Style::default().fg(Color::DarkGray));
        frame.render_widget(p, area);
        return;
    }

    // Group by owner
    let mut groups: Vec<(&str, Vec<(usize, &&RepoInfo)>)> = Vec::new();
    if group {
        let mut current_owner: Option<&str> = None;
        for (idx, repo) in repos.iter().enumerate() {
            let owner = repo.owner.as_str();
            if current_owner != Some(owner) {
                groups.push((owner, Vec::new()));
                current_owner = Some(owner);
            }
            groups.last_mut().unwrap().1.push((idx, repo));
        }
    } else {
        groups.push(("", repos.iter().enumerate().collect()));
    }

    let mut lines: Vec<Line> = Vec::new();
    let mut selected_line = 0usize;

    for (org, org_repos) in &groups {
        if group {
            lines.push(Line::from(Span::styled(
                format!("  {} ({})", org, org_repos.len()),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )));
        }

        let compact_mode = area.width < 76;
        let name_width = area.width.saturating_sub(37).max(14) as usize;
        if !compact_mode {
            lines.push(Line::from(vec![
                Span::styled("    ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!(
                        "{:<name_w$} {:>6} {:>6} {:>7} {:>6} {:>7}",
                        "Repository",
                        "Stars",
                        "Forks",
                        "Issues",
                        "PRs",
                        "Updated",
                        name_w = name_width
                    ),
                    Style::default()
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::BOLD),
                ),
            ]));
            lines.push(Line::from(Span::styled(
                "    ─────────────────────────────────────────────────────────────",
                Style::default().fg(Color::DarkGray),
            )));
        }

        for (flat_idx, repo) in org_repos {
            let prefix = if *flat_idx == selected { " > " } else { "   " };
            let visibility = if repo.is_private { "🔒" } else { "  " };
            let raw_name = repo.full_name.split('/').last().unwrap_or(&repo.full_name);
            let name = truncate_ellipsis(raw_name, name_width);
            let stars = compact_u32(repo.stargazers_count);
            let forks = compact_u32(repo.forks_count);
            let issues = compact_u32(
                repo.open_issues_only_count
                    .unwrap_or(repo.open_issues_count),
            );
            let prs = repo
                .open_prs_count
                .map(compact_u32)
                .unwrap_or_else(|| "-".to_string());
            let pushed = repo
                .pushed_at
                .map(|dt| {
                    let ago = Utc::now().signed_duration_since(dt);
                    if ago.num_hours() < 1 {
                        format!("{}m", ago.num_minutes())
                    } else if ago.num_hours() < 24 {
                        format!("{}h", ago.num_hours())
                    } else {
                        format!("{}d", ago.num_days())
                    }
                })
                .unwrap_or_default();

            let (style, row_bg) = if *flat_idx == selected {
                (
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                    Color::Rgb(98, 114, 98),
                )
            } else {
                (Style::default(), Color::Reset)
            };

            let line = if compact_mode {
                Line::from(vec![
                    Span::styled(prefix, style),
                    Span::raw(visibility),
                    Span::styled(format!(" {}", name), style),
                    Span::styled("  ", Style::default().fg(Color::DarkGray)),
                    Span::styled(format!("! {}", issues), Style::default().fg(Color::Red)),
                    Span::styled(" ", Style::default().fg(Color::DarkGray)),
                    Span::styled(format!("PR {}", prs), Style::default().fg(Color::Green)),
                    Span::styled(" ", Style::default().fg(Color::DarkGray)),
                    Span::styled(format!("★ {}", stars), Style::default().fg(Color::Yellow)),
                    Span::styled(" ", Style::default().fg(Color::DarkGray)),
                    Span::styled(format!("{} ", pushed), Style::default().fg(Color::DarkGray)),
                ])
                .patch_style(Style::default().bg(row_bg))
            } else {
                Line::from(vec![
                    Span::styled(prefix, style),
                    Span::raw(visibility),
                    Span::styled(
                        format!(
                            " {:<name_w$} {:>6} {:>6} {:>7} {:>6} {:>7}",
                            name,
                            stars,
                            forks,
                            issues,
                            prs,
                            pushed,
                            name_w = name_width
                        ),
                        style,
                    ),
                ])
                .patch_style(Style::default().bg(row_bg))
            };
            lines.push(line);

            if *flat_idx == selected {
                selected_line = lines.len().saturating_sub(1);
            }
        }
        if group {
            lines.push(Line::from(""));
        } // gap between groups
    }

    // Scroll: ensure selected item is visible
    let scroll = if selected_line >= area.height as usize {
        selected_line - area.height as usize + 2
    } else {
        0
    };

    let visible: Vec<Line> = lines
        .into_iter()
        .skip(scroll)
        .take(area.height as usize)
        .collect();
    frame.render_widget(Paragraph::new(visible), area);
}

fn find_visual_row(
    selected: usize,
    groups: &[(&str, Vec<(usize, &&RepoInfo)>)],
    num_cols: usize,
    group: bool,
) -> usize {
    let mut row = 0;
    for (_org, org_repos) in groups {
        if group {
            row += 1;
        } // org header
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
        3 => (
            '\u{2588}',
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        4 => (
            '\u{2588}',
            Style::default()
                .fg(Color::LightGreen)
                .add_modifier(Modifier::BOLD),
        ),
        _ => ('\u{2591}', Style::default().fg(Color::DarkGray)),
    }
}

fn calculate_streaks(days: &[ContributionDay]) -> (u32, u32) {
    let today = Utc::now().date_naive();
    let mut current = 0u32;
    for day in days.iter().rev() {
        if day.date > today {
            continue;
        }
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
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(ch);
    }
    result.chars().rev().collect()
}

fn compact_u32(n: u32) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

fn truncate_ellipsis(s: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    if s.len() <= width {
        return s.to_string();
    }
    let cut = s.floor_char_boundary(width.saturating_sub(1));
    format!("{}…", &s[..cut])
}

fn display_width(s: &str) -> usize {
    s.chars().count()
}

fn truncate_ellipsis_chars(s: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let len = s.chars().count();
    if len <= width {
        return s.to_string();
    }
    if width == 1 {
        return "…".to_string();
    }
    let keep = width - 1;
    let mut out = String::new();
    for ch in s.chars().take(keep) {
        out.push(ch);
    }
    out.push('…');
    out
}

fn split_desc_two_lines(desc: &str, width: usize) -> (String, String) {
    if width == 0 || desc.is_empty() {
        return (String::new(), String::new());
    }
    if desc.len() <= width {
        return (desc.to_string(), String::new());
    }

    let first_cut = desc.floor_char_boundary(width);
    let first = desc[..first_cut].to_string();
    let rest = desc[first_cut..].trim_start();
    if rest.is_empty() {
        return (first, String::new());
    }
    if rest.len() <= width {
        return (first, rest.to_string());
    }
    let second_cut = rest.floor_char_boundary(width.saturating_sub(1));
    (first, format!("{}…", &rest[..second_cut]))
}
