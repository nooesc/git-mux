# Repo Stats Bordered Panels Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace the flat header + heatmap with four bordered panels showing repo identity, health metrics, commit heatmap, and language breakdown — all in the same 10-line vertical budget.

**Architecture:** Three new GitHub API modules (languages, contributors, code_frequency) fetch data in parallel with existing calls. App state gains three new fields. The repo_detail.rs renderer replaces the flat header+heatmap with two rows of horizontal `Layout` splits, each cell wrapped in a ratatui `Block` with `Borders::ALL`.

**Tech Stack:** Rust, ratatui 0.29 (Block, Borders, Layout), octocrab (GitHub REST), tokio::join!, chrono

---

## Task 1: Add GitHub API — Languages

**Files:**
- Create: `src/github/languages.rs`
- Modify: `src/github/mod.rs:1-10`

**Step 1: Create `src/github/languages.rs`**

```rust
use anyhow::Result;
use super::GitHubClient;

impl GitHubClient {
    /// Fetch language breakdown for a repo.
    /// Returns vec of (language_name, bytes) sorted by bytes descending.
    pub async fn fetch_languages(&self, owner: &str, repo: &str) -> Result<Vec<(String, u64)>> {
        let result: serde_json::Value = self.octocrab.get(
            format!("/repos/{}/{}/languages", owner, repo),
            None::<&()>,
        ).await?;

        let mut langs: Vec<(String, u64)> = result
            .as_object()
            .map(|obj| {
                obj.iter()
                    .map(|(k, v)| (k.clone(), v.as_u64().unwrap_or(0)))
                    .collect()
            })
            .unwrap_or_default();

        langs.sort_by(|a, b| b.1.cmp(&a.1));
        Ok(langs)
    }
}
```

**Step 2: Add module declaration to `src/github/mod.rs`**

Add `pub mod languages;` after the existing `pub mod issues;` line (line 6).

**Step 3: Verify it compiles**

Run: `cargo check`
Expected: compiles with no errors

**Step 4: Commit**

```bash
git add src/github/languages.rs src/github/mod.rs
git commit -m "feat: add GitHub languages API module"
```

---

## Task 2: Add GitHub API — Contributors

**Files:**
- Create: `src/github/contributors.rs`
- Modify: `src/github/mod.rs`

**Step 1: Create `src/github/contributors.rs`**

Uses the same 202-retry pattern as `fetch_commit_activity` in `src/github/commits.rs:24-36`.

```rust
use anyhow::Result;
use super::GitHubClient;

#[derive(Debug, Clone)]
pub struct ContributorInfo {
    pub login: String,
    pub total_commits: u32,
}

impl GitHubClient {
    /// Fetch contributor stats for a repo.
    /// GitHub stats API returns 202 while computing; retry a few times.
    /// Returns vec of ContributorInfo sorted by total_commits descending.
    pub async fn fetch_contributors(&self, owner: &str, repo: &str) -> Result<Vec<ContributorInfo>> {
        let mut result = serde_json::Value::Null;
        for _ in 0..3 {
            result = self.octocrab.get(
                format!("/repos/{}/{}/stats/contributors", owner, repo),
                None::<&()>,
            ).await.unwrap_or(serde_json::Value::Null);
            if result.is_array() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }

        let mut contributors: Vec<ContributorInfo> = result
            .as_array()
            .map(|arr| {
                arr.iter().map(|item| {
                    ContributorInfo {
                        login: item["author"]["login"].as_str().unwrap_or("unknown").to_string(),
                        total_commits: item["total"].as_u64().unwrap_or(0) as u32,
                    }
                }).collect()
            })
            .unwrap_or_default();

        contributors.sort_by(|a, b| b.total_commits.cmp(&a.total_commits));
        Ok(contributors)
    }
}
```

**Step 2: Add module declaration to `src/github/mod.rs`**

Add `pub mod contributors;` after `pub mod contributions;` (line 5).

**Step 3: Verify it compiles**

Run: `cargo check`
Expected: compiles with no errors

**Step 4: Commit**

```bash
git add src/github/contributors.rs src/github/mod.rs
git commit -m "feat: add GitHub contributors stats API module"
```

---

## Task 3: Add GitHub API — Code Frequency

**Files:**
- Create: `src/github/code_frequency.rs`
- Modify: `src/github/mod.rs`

**Step 1: Create `src/github/code_frequency.rs`**

Uses the same 202-retry pattern. Returns weekly `(timestamp, additions, deletions)`.

```rust
use anyhow::Result;
use super::GitHubClient;

impl GitHubClient {
    /// Fetch weekly code frequency (additions/deletions) for a repo.
    /// GitHub stats API returns 202 while computing; retry a few times.
    /// Returns vec of (week_unix_timestamp, additions, deletions).
    pub async fn fetch_code_frequency(&self, owner: &str, repo: &str) -> Result<Vec<(i64, i64, i64)>> {
        let mut result = serde_json::Value::Null;
        for _ in 0..3 {
            result = self.octocrab.get(
                format!("/repos/{}/{}/stats/code_frequency", owner, repo),
                None::<&()>,
            ).await.unwrap_or(serde_json::Value::Null);
            if result.is_array() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }

        let weeks: Vec<(i64, i64, i64)> = result
            .as_array()
            .map(|arr| {
                arr.iter().filter_map(|item| {
                    let triple = item.as_array()?;
                    if triple.len() >= 3 {
                        Some((
                            triple[0].as_i64().unwrap_or(0),
                            triple[1].as_i64().unwrap_or(0),
                            triple[2].as_i64().unwrap_or(0),
                        ))
                    } else {
                        None
                    }
                }).collect()
            })
            .unwrap_or_default();

        Ok(weeks)
    }
}
```

**Step 2: Add module declaration to `src/github/mod.rs`**

Add `pub mod code_frequency;` after `pub mod ci;` (line 3).

**Step 3: Verify it compiles**

Run: `cargo check`
Expected: compiles with no errors

**Step 4: Commit**

```bash
git add src/github/code_frequency.rs src/github/mod.rs
git commit -m "feat: add GitHub code frequency stats API module"
```

---

## Task 4: Extend App State and Message Enum

**Files:**
- Modify: `src/app.rs:1-14` (imports)
- Modify: `src/app.rs:95-153` (AppState struct)
- Modify: `src/app.rs:155-193` (AppState::new)
- Modify: `src/app.rs:318-346` (Message enum — RepoDetailLoaded variant)
- Modify: `src/app.rs:410-423` (update handler — RepoDetailLoaded arm)
- Modify: `src/app.rs:590-604` (Select handler — clear new fields on repo enter)

**Step 1: Add import for ContributorInfo**

At line 7, after the `CommitInfo` import, add `ContributorInfo`:

```rust
use crate::github::commits::{CommitInfo, WeeklyCommitActivity};
use crate::github::contributors::ContributorInfo;
```

**Step 2: Add new state fields to AppState**

After `pub repo_readme: Option<String>,` (line 128), add:

```rust
    pub repo_languages: Vec<(String, u64)>,
    pub repo_contributors: Vec<ContributorInfo>,
    pub repo_code_frequency: Vec<(i64, i64, i64)>,
```

**Step 3: Initialize new fields in AppState::new()**

After `repo_readme: None,` (line 179), add:

```rust
            repo_languages: Vec::new(),
            repo_contributors: Vec::new(),
            repo_code_frequency: Vec::new(),
```

**Step 4: Extend Message::RepoDetailLoaded variant**

Add three new fields after `readme: Option<String>,` (line 345):

```rust
        languages: Vec<(String, u64)>,
        contributors: Vec<ContributorInfo>,
        code_frequency: Vec<(i64, i64, i64)>,
```

**Step 5: Update the RepoDetailLoaded handler in update()**

The current handler destructures the message at line 410. Update the destructure pattern and body:

```rust
        Message::RepoDetailLoaded { repo, prs, issues, ci, commits, commit_activity, readme, languages, contributors, code_frequency } => {
            if let Screen::RepoDetail { repo_full_name, .. } = &state.screen {
                if *repo_full_name == repo {
                    state.repo_prs = prs;
                    state.repo_issues = issues;
                    state.repo_ci = ci;
                    state.repo_commits = commits;
                    state.repo_commit_activity = commit_activity;
                    state.repo_readme = readme;
                    state.repo_languages = languages;
                    state.repo_contributors = contributors;
                    state.repo_code_frequency = code_frequency;
                    state.loading.remove("repo_detail");
                }
            }
        }
```

**Step 6: Clear new fields when entering a repo**

In the `Message::Select` handler, after `state.repo_readme = None;` (line 601), add:

```rust
                                state.repo_languages.clear();
                                state.repo_contributors.clear();
                                state.repo_code_frequency.clear();
```

**Step 7: Verify it compiles**

Run: `cargo check`
Expected: Error in `src/main.rs` because the `Message::RepoDetailLoaded` send is missing the new fields. That's expected — we'll fix it in Task 5.

**Step 8: Commit**

```bash
git add src/app.rs
git commit -m "feat: extend AppState and Message with languages, contributors, code_frequency"
```

---

## Task 5: Wire New API Calls Into Main Loop

**Files:**
- Modify: `src/main.rs:189-205` (the tokio::join! block and Message send)

**Step 1: Expand the tokio::join! from 6 to 9 calls**

Replace the current `tokio::join!` block (lines 189-205) with:

```rust
                                let (prs, issues, ci, commits, commit_activity, readme, languages, contributors, code_frequency) = tokio::join!(
                                    client.fetch_repo_prs(owner, name),
                                    client.fetch_repo_issues(owner, name),
                                    client.fetch_repo_ci(owner, name),
                                    client.fetch_repo_commits(owner, name),
                                    client.fetch_commit_activity(owner, name),
                                    client.fetch_readme(owner, name),
                                    client.fetch_languages(owner, name),
                                    client.fetch_contributors(owner, name),
                                    client.fetch_code_frequency(owner, name),
                                );
                                let _ = tx.send(Message::RepoDetailLoaded {
                                    repo: format!("{}/{}", owner, name),
                                    prs: prs.unwrap_or_default(),
                                    issues: issues.unwrap_or_default(),
                                    ci: ci.unwrap_or_default(),
                                    commits: commits.unwrap_or_default(),
                                    commit_activity: commit_activity.unwrap_or_default(),
                                    readme: readme.ok(),
                                    languages: languages.unwrap_or_default(),
                                    contributors: contributors.unwrap_or_default(),
                                    code_frequency: code_frequency.unwrap_or_default(),
                                });
```

**Step 2: Verify it compiles**

Run: `cargo check`
Expected: compiles with no errors

**Step 3: Run tests**

Run: `cargo test`
Expected: Tests fail because existing tests that construct `Message::RepoDetailLoaded` are missing the new fields. Fix them in Step 4.

**Step 4: Fix test compilation**

In `src/app.rs` tests, there should be no tests that directly construct `Message::RepoDetailLoaded` — the existing tests use `Message::ReposLoaded` instead. If any tests reference the old variant, add the three new fields with empty defaults:

```rust
languages: Vec::new(),
contributors: Vec::new(),
code_frequency: Vec::new(),
```

**Step 5: Run tests again**

Run: `cargo test`
Expected: All tests pass (35 tests)

**Step 6: Commit**

```bash
git add src/main.rs src/app.rs
git commit -m "feat: wire languages, contributors, code_frequency into parallel fetch"
```

---

## Task 6: Render Bordered Panels — Repo + Health (Row 1)

This is the main rendering task. It replaces the first 4 lines (flat header) with two bordered panels side by side.

**Files:**
- Modify: `src/ui/repo_detail.rs:10-77` (layout + header rendering)

**Step 1: Update the layout**

Replace the current 4-way vertical layout (lines 19-24) with a new structure. The total top area is 10 lines: 5 for row 1 (Repo + Health) and 5 for row 2 (Activity + Languages):

```rust
    let [top_row1, top_row2, tabs_area, content_area] = Layout::vertical([
        Constraint::Length(5),
        Constraint::Length(5),
        Constraint::Length(2),
        Constraint::Fill(1),
    ]).areas(area);
```

**Step 2: Split row 1 horizontally into Repo + Health boxes**

```rust
    let [repo_box_area, health_box_area] = Layout::horizontal([
        Constraint::Percentage(55),
        Constraint::Percentage(45),
    ]).areas(top_row1);
```

**Step 3: Render the Repo box**

Replace the current header rendering (lines 27-71) with:

```rust
    // ── Repo box ──
    let repo = state.repos.iter().find(|r| r.full_name == *repo_full_name);

    let mut repo_lines: Vec<Line> = vec![
        Line::from(vec![
            Span::styled(
                repo_full_name.to_string(),
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
        let desc = r.description.as_deref().unwrap_or("");
        // Truncate description to fit box width (box width - 2 for borders - 1 padding)
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

    let repo_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(Span::styled(" Repo ", Style::default().fg(Color::Cyan)));

    frame.render_widget(Paragraph::new(repo_lines).block(repo_block), repo_box_area);
```

**Step 4: Render the Health box**

```rust
    // ── Health box ──
    let contributor_count = state.repo_contributors.len();
    let top_contributors: Vec<String> = state.repo_contributors.iter()
        .take(3)
        .map(|c| format!("{} ({})", c.login, c.total_commits))
        .collect();

    // Compute avg merge time from merged PRs
    let merged_prs: Vec<&PrInfo> = state.repo_prs.iter()
        .filter(|pr| pr.merged)
        .collect();
    let avg_merge = if merged_prs.is_empty() {
        "--".to_string()
    } else {
        let total_days: f64 = merged_prs.iter().filter_map(|pr| {
            let created = pr.created_at?;
            let updated = pr.updated_at?;
            Some(updated.signed_duration_since(created).num_hours() as f64 / 24.0)
        }).sum();
        let count = merged_prs.len() as f64;
        format!("{:.1}d", total_days / count)
    };

    // Compute issue close rate
    let total_issues = state.repo_issues.len();
    let closed_issues = state.repo_issues.iter().filter(|i| i.state == "closed").count();
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
            Span::styled(format!("{}", contributor_count), Style::default().fg(Color::Cyan)),
        ]),
        Line::from(if top_contributors.is_empty() {
            vec![Span::styled("  No data", Style::default().fg(Color::DarkGray))]
        } else {
            vec![Span::styled(format!(" {}", top_contributors.join("  ")), Style::default().fg(Color::Magenta))]
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
        .title(Span::styled(" Health ", Style::default().fg(Color::Cyan)));

    frame.render_widget(Paragraph::new(health_lines).block(health_block), health_box_area);
```

**Step 5: Verify it compiles**

Run: `cargo check`
Expected: compiles (will need to add `use ratatui::widgets::{Block, Borders}` import if not already present)

**Step 6: Commit**

```bash
git add src/ui/repo_detail.rs
git commit -m "feat: render Repo and Health bordered panels (row 1)"
```

---

## Task 7: Render Bordered Panels — Activity + Languages (Row 2)

**Files:**
- Modify: `src/ui/repo_detail.rs` (add row 2 rendering, refactor heatmap into Activity box)

**Step 1: Split row 2 horizontally into Activity + Languages boxes**

After the Health box rendering, add:

```rust
    let [activity_box_area, languages_box_area] = Layout::horizontal([
        Constraint::Percentage(55),
        Constraint::Percentage(45),
    ]).areas(top_row2);
```

**Step 2: Render the Activity box**

Refactor the existing `render_repo_heatmap` to render inside a Block. The heatmap now has exactly 3 inner lines (Mon/Wed/Fri). The "204 commits" and code frequency stats go in the block title.

```rust
    // ── Activity box ──
    let total_commits: u32 = state.repo_commit_activity.iter().map(|w| w.total).sum();

    // Code frequency: sum last 4 weeks of additions and deletions
    let recent_freq: (i64, i64) = state.repo_code_frequency.iter()
        .rev()
        .take(4)
        .fold((0i64, 0i64), |acc, &(_, add, del)| (acc.0 + add, acc.1 + del));

    let freq_str = if recent_freq.0 != 0 || recent_freq.1 != 0 {
        format!(" · +{} / {}", format_count(recent_freq.0), format_count(recent_freq.1))
    } else {
        String::new()
    };

    let activity_title = Line::from(vec![
        Span::styled(" Activity", Style::default().fg(Color::Cyan)),
        Span::styled(format!(" · {} commits", total_commits), Style::default().fg(Color::DarkGray)),
        if recent_freq.0 != 0 || recent_freq.1 != 0 {
            Span::styled(" · ", Style::default().fg(Color::DarkGray))
        } else {
            Span::raw("")
        },
        if recent_freq.0 != 0 {
            Span::styled(format!("+{}", format_count(recent_freq.0)), Style::default().fg(Color::Green))
        } else {
            Span::raw("")
        },
        if recent_freq.1 != 0 {
            Span::styled(format!(" / {}", format_count(recent_freq.1)), Style::default().fg(Color::Red))
        } else {
            Span::raw("")
        },
        Span::raw(" "),
    ]);

    let activity_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(activity_title);

    // Render heatmap lines inside the block
    let heatmap_inner = activity_box_area.inner(ratatui::layout::Margin { horizontal: 1, vertical: 1 });
    frame.render_widget(activity_block, activity_box_area);

    if !state.repo_commit_activity.is_empty() {
        let heatmap_lines = build_heatmap_lines(&state.repo_commit_activity, heatmap_inner.width);
        frame.render_widget(Paragraph::new(heatmap_lines), heatmap_inner);
    }
```

**Step 3: Render the Languages box**

```rust
    // ── Languages box ──
    let lang_lines = if state.repo_languages.is_empty() {
        vec![
            Line::from(Span::styled("No language data", Style::default().fg(Color::DarkGray))),
            Line::from(""),
            Line::from(""),
        ]
    } else {
        let total_bytes: u64 = state.repo_languages.iter().map(|(_, b)| *b).sum();
        let bar_width = 20usize;

        state.repo_languages.iter().take(3).map(|(name, bytes)| {
            let pct = if total_bytes > 0 { (*bytes as f64 / total_bytes as f64 * 100.0) } else { 0.0 };
            let filled = (pct / 100.0 * bar_width as f64).round() as usize;
            let empty = bar_width.saturating_sub(filled);

            Line::from(vec![
                Span::styled("█".repeat(filled), Style::default().fg(Color::Green)),
                Span::styled("░".repeat(empty), Style::default().fg(Color::DarkGray)),
                Span::raw(" "),
                Span::styled(format!("{:<12}", name), Style::default().fg(Color::White)),
                Span::styled(format!("{:>3.0}%", pct), Style::default().fg(Color::DarkGray)),
            ])
        }).collect()
    };

    let languages_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(Span::styled(" Languages ", Style::default().fg(Color::Cyan)));

    frame.render_widget(Paragraph::new(lang_lines).block(languages_block), languages_box_area);
```

**Step 4: Add helper functions**

Add at the bottom of the file:

```rust
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
    let max_daily = activity.iter()
        .flat_map(|w| w.days.iter())
        .copied()
        .max()
        .unwrap_or(1)
        .max(1);

    let label_width = 4usize; // "Mon " etc
    let max_weeks = (width as usize).saturating_sub(label_width);
    let visible = if activity.len() > max_weeks {
        &activity[activity.len() - max_weeks..]
    } else {
        activity
    };

    let day_rows = [(1, "Mon"), (3, "Wed"), (5, "Fri")];
    day_rows.iter().map(|(day_idx, day_label)| {
        let mut spans: Vec<Span> = vec![Span::styled(
            format!("{} ", day_label),
            Style::default().fg(Color::DarkGray),
        )];
        for week in visible {
            let count = week.days[*day_idx];
            let level = if count == 0 { 0 }
                else if count <= max_daily / 4 { 1 }
                else if count <= max_daily / 2 { 2 }
                else if count <= max_daily * 3 / 4 { 3 }
                else { 4 };
            let (ch, style) = level_to_cell(level);
            spans.push(Span::styled(String::from(ch), style));
        }
        Line::from(spans)
    }).collect()
}
```

**Step 5: Remove the old `render_repo_heatmap` function**

Delete the old `render_repo_heatmap` function (currently at lines 538-593) since its logic has been moved into `build_heatmap_lines` + the inline Activity box rendering. Keep the `level_to_cell` function.

**Step 6: Remove the old heatmap call**

Remove the old heatmap rendering block (the `if has_activity { render_repo_heatmap(...) }` block and the `has_activity` variable).

**Step 7: Verify it compiles**

Run: `cargo check`
Expected: compiles with no errors

**Step 8: Run tests**

Run: `cargo test`
Expected: All tests pass

**Step 9: Commit**

```bash
git add src/ui/repo_detail.rs
git commit -m "feat: render Activity and Languages bordered panels (row 2)"
```

---

## Task 8: Final Polish — Tests, Clippy, Smoke Test

**Files:**
- Modify: `src/app.rs` (add test for new state fields if needed)
- All files (clippy)

**Step 1: Run clippy**

Run: `cargo clippy -- -W clippy::all`
Expected: No warnings in the files we changed. Fix any warnings.

**Step 2: Run all tests**

Run: `cargo test`
Expected: All tests pass

**Step 3: Manual smoke test**

Run: `cargo run`
Expected:
- Navigate to any repo detail page
- See 4 bordered panels at top (Repo, Health, Activity, Languages)
- Repo box shows name, description, metadata
- Health box shows contributor count, top 3 contributors, avg merge time, close rate
- Activity box shows heatmap with commit count and code frequency in title
- Languages box shows top 3 languages with proportional bars
- Tab bar and content area below work as before

**Step 4: Commit any polish fixes**

```bash
git add -A
git commit -m "fix: clippy warnings and polish for repo stats panels"
```
