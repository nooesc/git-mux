use chrono::Utc;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use crate::app::{AppState, DetailFocus, RepoSection};

pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
    let (repo_full_name, section) = match &state.screen {
        crate::app::Screen::RepoDetail {
            repo_full_name,
            section,
        } => (repo_full_name, *section),
        _ => return,
    };

    let [top_row1, top_row2, tabs_area, content_area] = Layout::vertical([
        Constraint::Length(5),
        Constraint::Length(5),
        Constraint::Length(2),
        Constraint::Fill(1),
    ])
    .areas(area);

    let [repo_box_area, health_box_area] =
        Layout::horizontal([Constraint::Percentage(55), Constraint::Percentage(45)])
            .areas(top_row1);

    let [activity_box_area, languages_box_area] =
        Layout::horizontal([Constraint::Percentage(55), Constraint::Percentage(45)])
            .areas(top_row2);

    // ── Repo box ──
    let repo = state.repos.iter().find(|r| r.full_name == *repo_full_name);
    let loading_fast = state.loading.contains("repo_detail_fast");
    let loading_stats = state.loading.contains("repo_detail_stats");

    let mut repo_lines: Vec<Line> = vec![Line::from(vec![
        Span::styled(
            repo_full_name.to_string(),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            if repo.is_some_and(|r| r.is_private) {
                "🔒 Private"
            } else {
                "Public"
            },
            Style::default().fg(Color::DarkGray),
        ),
    ])];

    if let Some(r) = repo {
        let desc = r.description.as_deref().unwrap_or("");
        let max_desc = repo_box_area.width.saturating_sub(3) as usize;
        let truncated = if desc.len() > max_desc {
            format!("{}...", &desc[..max_desc.saturating_sub(3)])
        } else {
            desc.to_string()
        };
        repo_lines.push(Line::from(Span::styled(
            truncated,
            Style::default().fg(Color::DarkGray),
        )));
        repo_lines.push(Line::from(vec![
            Span::styled(
                format!("{} ", r.language.as_deref().unwrap_or("")),
                Style::default().fg(Color::Magenta),
            ),
            Span::styled(
                format!("· ★ {} ", r.stargazers_count),
                Style::default().fg(Color::Yellow),
            ),
            Span::styled(
                format!("· ⎚ {} ", r.forks_count),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(
                format!("· ⚠ {} ", r.open_issues_count),
                Style::default().fg(Color::DarkGray),
            ),
            Span::raw("· "),
            Span::styled(
                r.pushed_at
                    .map(|dt| {
                        let ago = Utc::now().signed_duration_since(dt);
                        if ago.num_hours() < 1 {
                            format!("pushed {}m ago", ago.num_minutes())
                        } else if ago.num_hours() < 24 {
                            format!("pushed {}h ago", ago.num_hours())
                        } else {
                            format!("pushed {}d ago", ago.num_days())
                        }
                    })
                    .unwrap_or_default(),
                Style::default().fg(Color::DarkGray),
            ),
        ]));
    }

    let mut repo_title = " Repo ".to_string();
    if let Some(cached_at) = state.detail_cache_saved_at {
        let age = Utc::now().signed_duration_since(cached_at);
        let age_text = if age.num_minutes() < 1 {
            "just now".to_string()
        } else if age.num_minutes() < 60 {
            format!("{}m ago", age.num_minutes())
        } else {
            format!("{}h ago", age.num_hours())
        };
        repo_title = format!(" Repo · cached {} ", age_text);
    } else if loading_fast || loading_stats {
        repo_title = " Repo · refreshing ".to_string();
    }

    let repo_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(Span::styled(repo_title, Style::default().fg(Color::Cyan)));
    frame.render_widget(Paragraph::new(repo_lines).block(repo_block), repo_box_area);

    // ── Health box ──
    let contributor_count = state.repo_contributors.len();
    let top_contributors: Vec<String> = state
        .repo_contributors
        .iter()
        .take(3)
        .map(|c| format!("{} ({})", c.login, c.total_commits))
        .collect();

    let merged_prs: Vec<&crate::github::prs::PrInfo> =
        state.repo_prs.iter().filter(|pr| pr.merged).collect();
    let avg_merge = if merged_prs.is_empty() {
        "--".to_string()
    } else {
        let total_days: f64 = merged_prs
            .iter()
            .filter_map(|pr| {
                let created = pr.created_at?;
                let updated = pr.updated_at?;
                Some(updated.signed_duration_since(created).num_hours() as f64 / 24.0)
            })
            .sum();
        let count = merged_prs.len() as f64;
        format!("{:.1}d", total_days / count)
    };

    let total_issues = state.repo_issues.len();
    let closed_issues = state
        .repo_issues
        .iter()
        .filter(|i| i.state == "closed")
        .count();
    let close_rate = if total_issues == 0 {
        "--".to_string()
    } else {
        format!("{}%", closed_issues * 100 / total_issues)
    };
    let close_rate_color = if total_issues == 0 {
        Color::DarkGray
    } else if closed_issues * 100 / total_issues.max(1) > 50 {
        Color::Green
    } else {
        Color::Red
    };

    let health_lines = vec![
        Line::from(vec![
            Span::styled("Contributors: ", Style::default().fg(Color::White)),
            Span::styled(
                format!("{}", contributor_count),
                Style::default().fg(Color::Cyan),
            ),
        ]),
        Line::from(if top_contributors.is_empty() {
            vec![Span::styled(
                if loading_stats {
                    " Loading contributors..."
                } else {
                    " No data"
                },
                Style::default().fg(Color::DarkGray),
            )]
        } else {
            vec![Span::styled(
                format!(" {}", top_contributors.join("  ")),
                Style::default().fg(Color::Magenta),
            )]
        }),
        Line::from(vec![
            Span::styled("Avg merge: ", Style::default().fg(Color::White)),
            Span::styled(&avg_merge, Style::default().fg(Color::Yellow)),
            Span::raw("  "),
            Span::styled("Close rate: ", Style::default().fg(Color::White)),
            Span::styled(&close_rate, Style::default().fg(close_rate_color)),
        ]),
    ];

    let health_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(Span::styled(
            if loading_stats {
                " Health · loading "
            } else {
                " Health "
            },
            Style::default().fg(Color::Cyan),
        ));
    frame.render_widget(
        Paragraph::new(health_lines).block(health_block),
        health_box_area,
    );

    // ── Activity box ──
    let total_commits: u32 = state.repo_commit_activity.iter().map(|w| w.total).sum();
    let recent_freq: (i64, i64) = state
        .repo_code_frequency
        .iter()
        .rev()
        .take(4)
        .fold((0i64, 0i64), |acc, &(_, add, del)| {
            (acc.0 + add, acc.1 + del)
        });

    let mut activity_title_spans = vec![
        Span::styled(" Activity", Style::default().fg(Color::Cyan)),
        Span::styled(
            format!(" · {} commits", total_commits),
            Style::default().fg(Color::DarkGray),
        ),
    ];
    if loading_stats && state.repo_commit_activity.is_empty() {
        activity_title_spans.push(Span::styled(
            " · loading",
            Style::default().fg(Color::DarkGray),
        ));
    }
    if recent_freq.0 != 0 || recent_freq.1 != 0 {
        activity_title_spans.push(Span::styled(" · ", Style::default().fg(Color::DarkGray)));
        activity_title_spans.push(Span::styled(
            format!("+{}", format_count(recent_freq.0)),
            Style::default().fg(Color::Green),
        ));
        activity_title_spans.push(Span::styled(
            format!(" / {}", format_count(recent_freq.1)),
            Style::default().fg(Color::Red),
        ));
    }
    activity_title_spans.push(Span::raw(" "));

    let activity_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(Line::from(activity_title_spans));

    // Render the block first, then heatmap inside
    let activity_inner = activity_block.inner(activity_box_area);
    frame.render_widget(activity_block, activity_box_area);

    if !state.repo_commit_activity.is_empty() {
        let heatmap_lines = build_heatmap_lines(&state.repo_commit_activity, activity_inner.width);
        frame.render_widget(Paragraph::new(heatmap_lines), activity_inner);
    } else if loading_stats {
        frame.render_widget(
            Paragraph::new(" Loading activity...").style(Style::default().fg(Color::DarkGray)),
            activity_inner,
        );
    }

    // ── Languages box ──
    let lang_lines: Vec<Line> = if state.repo_languages.is_empty() {
        vec![
            Line::from(Span::styled(
                if loading_fast {
                    "Loading language data..."
                } else {
                    "No language data"
                },
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(""),
            Line::from(""),
        ]
    } else {
        let total_bytes: u64 = state.repo_languages.iter().map(|(_, b)| *b).sum();
        let bar_width = 20usize;

        state
            .repo_languages
            .iter()
            .take(3)
            .map(|(name, bytes)| {
                let pct = if total_bytes > 0 {
                    *bytes as f64 / total_bytes as f64 * 100.0
                } else {
                    0.0
                };
                let filled = (pct / 100.0 * bar_width as f64).round() as usize;
                let empty = bar_width.saturating_sub(filled);

                Line::from(vec![
                    Span::styled("█".repeat(filled), Style::default().fg(Color::Green)),
                    Span::styled("░".repeat(empty), Style::default().fg(Color::DarkGray)),
                    Span::raw(" "),
                    Span::styled(format!("{:<12}", name), Style::default().fg(Color::White)),
                    Span::styled(
                        format!("{:>3.0}%", pct),
                        Style::default().fg(Color::DarkGray),
                    ),
                ])
            })
            .collect()
    };

    let languages_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(Span::styled(
            " Languages ",
            Style::default().fg(Color::Cyan),
        ));
    frame.render_widget(
        Paragraph::new(lang_lines).block(languages_block),
        languages_box_area,
    );

    // ── Section tabs ──
    let focused = state.detail_focus == DetailFocus::TabBar;
    let tabs: Vec<(RepoSection, String)> = vec![
        (
            RepoSection::PRs,
            format!(
                "PRs ({}){}",
                state.repo_prs.len(),
                if loading_fast { "…" } else { "" }
            ),
        ),
        (
            RepoSection::Issues,
            format!(
                "Issues ({}){}",
                state.repo_issues.len(),
                if loading_fast { "…" } else { "" }
            ),
        ),
        (
            RepoSection::CI,
            format!(
                "CI ({}){}",
                state.repo_ci.len(),
                if loading_fast { "…" } else { "" }
            ),
        ),
        (
            RepoSection::Commits,
            format!(
                "Commits ({}){}",
                state.repo_commits.len(),
                if loading_fast { "…" } else { "" }
            ),
        ),
        (
            RepoSection::Info,
            if loading_fast {
                "Info…".to_string()
            } else {
                "Info".to_string()
            },
        ),
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
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else if active {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        tab_spans.push(Span::styled(label.clone(), style));
        let bar: String = if active {
            "━".repeat(width)
        } else {
            "─".repeat(width)
        };
        let bar_style = if active {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        underline_spans.push(Span::styled(bar, bar_style));
    }

    frame.render_widget(
        Paragraph::new(vec![Line::from(tab_spans), Line::from(underline_spans)]),
        tabs_area,
    );

    // ── Content ──
    let section_has_data = match section {
        RepoSection::PRs => !state.repo_prs.is_empty(),
        RepoSection::Issues => !state.repo_issues.is_empty(),
        RepoSection::CI => !state.repo_ci.is_empty(),
        RepoSection::Commits => !state.repo_commits.is_empty(),
        RepoSection::Info => state.repo_readme.is_some(),
    };
    if loading_fast && !section_has_data {
        let p = Paragraph::new("  Loading...").style(Style::default().fg(Color::DarkGray));
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
    let mut selected_line_end = 0usize;

    for (idx, item) in items.iter().enumerate() {
        match item {
            crate::app::DetailItem::SectionHeader(label) => {
                let w = area.width as usize;
                let pad = w.saturating_sub(label.len() + 5);
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("── {} {}", label, "─".repeat(pad)),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]));
            }
            crate::app::DetailItem::Pr(pr) => {
                let selected = idx == state.detail_selected;
                let icon = if pr.draft {
                    "○"
                } else if pr.merged {
                    "◆"
                } else if pr.state == "closed" {
                    "●"
                } else {
                    "●"
                };
                let icon_color = if pr.draft {
                    Color::DarkGray
                } else if pr.merged {
                    Color::Magenta
                } else if pr.state == "closed" {
                    Color::Red
                } else {
                    Color::Green
                };

                let style = if selected {
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };

                let age = pr
                    .updated_at
                    .map(|dt| {
                        let ago = Utc::now().signed_duration_since(dt);
                        if ago.num_hours() < 1 {
                            format!("{}m ago", ago.num_minutes())
                        } else if ago.num_hours() < 24 {
                            format!("{}h ago", ago.num_hours())
                        } else {
                            format!("{}d ago", ago.num_days())
                        }
                    })
                    .unwrap_or_default();

                lines.push(Line::from(vec![
                    Span::raw(if selected { "  > " } else { "    " }),
                    Span::styled(icon, Style::default().fg(icon_color)),
                    Span::styled(
                        format!(" #{:<5}", pr.number),
                        Style::default().fg(Color::DarkGray),
                    ),
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
                    Span::styled(
                        format!("{} ← {}", pr.base_ref, pr.head_ref),
                        Style::default().fg(Color::DarkGray),
                    ),
                    Span::raw("  "),
                    Span::styled(age, Style::default().fg(Color::DarkGray)),
                    Span::styled(diff_stats, Style::default().fg(Color::Green)),
                ]));
                lines.push(Line::from(""));
            }
            _ => {}
        }

        if idx == state.detail_selected {
            selected_line_end = lines.len();
        }
    }

    // Scroll to keep selected item visible
    let visible = area.height as usize;
    let scroll = if selected_line_end > visible {
        selected_line_end - visible
    } else {
        0
    };

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
    let mut selected_line_end = 0usize;

    for (idx, item) in items.iter().enumerate() {
        match item {
            crate::app::DetailItem::SectionHeader(label) => {
                let w = area.width as usize;
                let pad = w.saturating_sub(label.len() + 5);
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("── {} {}", label, "─".repeat(pad)),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]));
            }
            crate::app::DetailItem::Issue(issue) => {
                let selected = idx == state.detail_selected;
                let icon = if issue.state == "closed" {
                    "●"
                } else {
                    "○"
                };
                let icon_color = if issue.state == "closed" {
                    Color::Red
                } else {
                    Color::Green
                };

                let style = if selected {
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };

                let labels_str = if issue.labels.is_empty() {
                    String::new()
                } else {
                    format!(" [{}]", issue.labels.join(", "))
                };

                let age = issue
                    .updated_at
                    .map(|dt| {
                        let ago = Utc::now().signed_duration_since(dt);
                        if ago.num_hours() < 1 {
                            format!("{}m ago", ago.num_minutes())
                        } else if ago.num_hours() < 24 {
                            format!("{}h ago", ago.num_hours())
                        } else {
                            format!("{}d ago", ago.num_days())
                        }
                    })
                    .unwrap_or_default();

                lines.push(Line::from(vec![
                    Span::raw(if selected { "  > " } else { "    " }),
                    Span::styled(icon, Style::default().fg(icon_color)),
                    Span::styled(
                        format!(" #{:<5}", issue.number),
                        Style::default().fg(Color::DarkGray),
                    ),
                    Span::styled(&issue.title, style),
                    Span::styled(labels_str, Style::default().fg(Color::Yellow)),
                ]));
                lines.push(Line::from(vec![
                    Span::raw("         "),
                    Span::styled(&issue.user, Style::default().fg(Color::Magenta)),
                    Span::raw("  "),
                    Span::styled(age, Style::default().fg(Color::DarkGray)),
                    Span::raw("  "),
                    Span::styled(
                        format!("💬 {}", issue.comments),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]));
                lines.push(Line::from(""));
            }
            _ => {}
        }

        if idx == state.detail_selected {
            selected_line_end = lines.len();
        }
    }

    let visible = area.height as usize;
    let scroll = if selected_line_end > visible {
        selected_line_end - visible
    } else {
        0
    };
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
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            let duration = run
                .duration_secs
                .map(|s| {
                    if s < 60 {
                        format!("{}s", s)
                    } else {
                        format!("{}m{}s", s / 60, s % 60)
                    }
                })
                .unwrap_or_default();

            let age = run
                .created_at
                .map(|dt| {
                    let ago = Utc::now().signed_duration_since(dt);
                    if ago.num_hours() < 1 {
                        format!("{}m ago", ago.num_minutes())
                    } else if ago.num_hours() < 24 {
                        format!("{}h ago", ago.num_hours())
                    } else {
                        format!("{}d ago", ago.num_days())
                    }
                })
                .unwrap_or_default();

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
    let scroll = if selected_top + item_height > visible {
        selected_top + item_height - visible
    } else {
        0
    };
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
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
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
                if ago.num_hours() < 1 {
                    format!("{}m ago", ago.num_minutes())
                } else if ago.num_hours() < 24 {
                    format!("{}h ago", ago.num_hours())
                } else {
                    format!("{}d ago", ago.num_days())
                }
            };

            lines.push(Line::from(vec![
                Span::raw(if is_merge {
                    "       |\\  "
                } else {
                    "       |  "
                }),
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
    let scroll = if selected_top + item_height > visible {
        selected_top + item_height - visible
    } else {
        0
    };
    let visible_lines: Vec<Line> = lines.into_iter().skip(scroll).take(visible).collect();
    frame.render_widget(Paragraph::new(visible_lines), area);
}

fn render_readme(frame: &mut Frame, area: Rect, state: &AppState) {
    let readme_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(Span::styled(" README ", Style::default().fg(Color::Cyan)));
    let inner = readme_block.inner(area);
    frame.render_widget(readme_block, area);

    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let content = match &state.repo_readme {
        Some(c) => c,
        None => {
            frame.render_widget(
                Paragraph::new("  No README available").style(Style::default().fg(Color::DarkGray)),
                inner,
            );
            return;
        }
    };

    let mut lines: Vec<Line> = Vec::new();
    let mut in_code_block = false;

    for raw_line in content.lines() {
        let trimmed = raw_line.trim();

        // Code block toggle
        if let Some(fence) = trimmed.strip_prefix("```") {
            let lang = fence.trim();
            if !in_code_block {
                let label = if lang.is_empty() {
                    "  ┌─ code".to_string()
                } else {
                    format!("  ┌─ code ({})", lang)
                };
                lines.push(Line::from(Span::styled(
                    label,
                    Style::default().fg(Color::DarkGray),
                )));
            } else {
                lines.push(Line::from(Span::styled(
                    "  └─ end code",
                    Style::default().fg(Color::DarkGray),
                )));
            }
            in_code_block = !in_code_block;
            continue;
        }

        if in_code_block {
            lines.push(Line::from(Span::styled(
                format!("  │ {}", raw_line),
                Style::default().fg(Color::Green),
            )));
            continue;
        }

        // Headers
        if let Some(h) = trimmed.strip_prefix("### ") {
            if !lines.is_empty() {
                lines.push(Line::from(""));
            }
            lines.push(Line::from(Span::styled(
                format!("  {}", h),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )));
        } else if let Some(h) = trimmed.strip_prefix("## ") {
            if !lines.is_empty() {
                lines.push(Line::from(""));
            }
            lines.push(Line::from(Span::styled(
                format!("  {}", h),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(Span::styled(
                "  ─────────────────────────────────",
                Style::default().fg(Color::DarkGray),
            )));
        } else if let Some(h) = trimmed.strip_prefix("# ") {
            if !lines.is_empty() {
                lines.push(Line::from(""));
            }
            lines.push(Line::from(Span::styled(
                format!("  {}", h),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(Span::styled(
                "  ═════════════════════════════════",
                Style::default().fg(Color::DarkGray),
            )));
        }
        // List items
        else if trimmed.starts_with("- [ ] ") || trimmed.starts_with("* [ ] ") {
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled("☐ ", Style::default().fg(Color::DarkGray)),
                Span::raw(trimmed[6..].to_string()),
            ]));
        } else if trimmed.starts_with("- [x] ")
            || trimmed.starts_with("* [x] ")
            || trimmed.starts_with("- [X] ")
            || trimmed.starts_with("* [X] ")
        {
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled("☑ ", Style::default().fg(Color::Green)),
                Span::raw(trimmed[6..].to_string()),
            ]));
        } else if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled("• ", Style::default().fg(Color::Cyan)),
                Span::raw(trimmed[2..].to_string()),
            ]));
        } else if trimmed.starts_with("+ ") {
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled("• ", Style::default().fg(Color::Cyan)),
                Span::raw(trimmed[2..].to_string()),
            ]));
        } else if let Some((marker, item)) = parse_ordered_marker(trimmed) {
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(format!("{} ", marker), Style::default().fg(Color::Cyan)),
                Span::raw(item.to_string()),
            ]));
        } else if let Some(quote) = trimmed.strip_prefix("> ") {
            lines.push(Line::from(vec![
                Span::styled("  ▌ ", Style::default().fg(Color::DarkGray)),
                Span::styled(quote.to_string(), Style::default().fg(Color::DarkGray)),
            ]));
        } else if is_horizontal_rule(trimmed) {
            lines.push(Line::from(Span::styled(
                "  ─────────────────────────────────",
                Style::default().fg(Color::DarkGray),
            )));
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
    let visible = inner.height as usize;
    let max_scroll = lines.len().saturating_sub(visible);
    let scroll = state.detail_selected.min(max_scroll);
    let visible_lines: Vec<Line> = lines.into_iter().skip(scroll).take(visible).collect();
    frame.render_widget(
        Paragraph::new(visible_lines).wrap(Wrap { trim: false }),
        inner,
    );
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
                    if c == '`' {
                        chars.next();
                        break;
                    }
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
                    if c == '*' && chars.peek() == Some(&'*') {
                        chars.next();
                        break;
                    }
                    bold.push(c);
                }
                spans.push(Span::styled(
                    bold,
                    Style::default().add_modifier(Modifier::BOLD),
                ));
            }
            '*' => {
                if !buf.is_empty() {
                    spans.push(Span::raw(buf.clone()));
                    buf.clear();
                }
                let mut italic = String::new();
                for c in chars.by_ref() {
                    if c == '*' {
                        break;
                    }
                    italic.push(c);
                }
                spans.push(Span::styled(
                    italic,
                    Style::default().add_modifier(Modifier::ITALIC),
                ));
            }
            '[' => {
                if !buf.is_empty() {
                    spans.push(Span::raw(buf.clone()));
                    buf.clear();
                }
                let mut link_text = String::new();
                for c in chars.by_ref() {
                    if c == ']' {
                        break;
                    }
                    link_text.push(c);
                }
                let mut link_url = String::new();
                if chars.peek() == Some(&'(') {
                    chars.next();
                    for c in chars.by_ref() {
                        if c == ')' {
                            break;
                        }
                        link_url.push(c);
                    }
                }
                spans.push(Span::styled(
                    link_text,
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::UNDERLINED),
                ));
                if !link_url.is_empty() {
                    spans.push(Span::styled(
                        format!(" ({})", link_url),
                        Style::default().fg(Color::DarkGray),
                    ));
                }
            }
            _ => buf.push(ch),
        }
    }
    if !buf.is_empty() {
        spans.push(Span::raw(buf));
    }
    Line::from(spans)
}

fn parse_ordered_marker(line: &str) -> Option<(&str, &str)> {
    let dot_idx = line.find(". ")?;
    if dot_idx == 0 {
        return None;
    }
    let marker = &line[..dot_idx + 1];
    if marker.chars().all(|c| c.is_ascii_digit() || c == '.') {
        Some((marker, &line[dot_idx + 2..]))
    } else {
        None
    }
}

fn is_horizontal_rule(line: &str) -> bool {
    if line.len() < 3 {
        return false;
    }
    let mut chars = line.chars();
    let first = chars.next().unwrap_or_default();
    if first != '-' && first != '*' && first != '_' {
        return false;
    }
    line.chars().all(|c| c == first)
}

/// Format a number with k/M suffix for compact display.
fn format_count(n: i64) -> String {
    let abs = n.unsigned_abs();
    if abs >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if abs >= 1_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        format!("{}", n)
    }
}

/// Build heatmap lines (Mon/Wed/Fri) for the activity box.
fn build_heatmap_lines<'a>(
    activity: &[crate::github::commits::WeeklyCommitActivity],
    width: u16,
) -> Vec<Line<'a>> {
    let max_daily = activity
        .iter()
        .flat_map(|w| w.days.iter())
        .copied()
        .max()
        .unwrap_or(1)
        .max(1);

    let label_width = 4usize;
    let max_weeks = (width as usize).saturating_sub(label_width);
    let visible = if activity.len() > max_weeks {
        &activity[activity.len() - max_weeks..]
    } else {
        activity
    };

    let day_rows = [(1, "Mon"), (3, "Wed"), (5, "Fri")];
    day_rows
        .iter()
        .map(|(day_idx, day_label)| {
            let mut spans: Vec<Span> = vec![Span::styled(
                format!("{} ", day_label),
                Style::default().fg(Color::DarkGray),
            )];
            for week in visible {
                let count = week.days[*day_idx];
                let level = if count == 0 {
                    0
                } else if count <= max_daily / 4 {
                    1
                } else if count <= max_daily / 2 {
                    2
                } else if count <= max_daily * 3 / 4 {
                    3
                } else {
                    4
                };
                let (ch, style) = level_to_cell(level);
                spans.push(Span::styled(String::from(ch), style));
            }
            Line::from(spans)
        })
        .collect()
}

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
