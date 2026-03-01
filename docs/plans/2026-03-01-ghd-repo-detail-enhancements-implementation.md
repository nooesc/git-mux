# GHD Repo Detail Enhancements Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a persistent repo-scoped commit heatmap, a Commits tab with branch/merge graph, and an Info tab with rendered README to the repo detail page.

**Architecture:** Two new GitHub API modules (commits, readme) fetch data in parallel with existing PRs/Issues/CI. `RepoSection` expands from 3 to 5 variants. The heatmap renders above the tab bar on every section. README is parsed line-by-line into styled ratatui spans.

**Tech Stack:** Rust, ratatui 0.29, octocrab (GitHub API), chrono, base64, serde_json

---

### Task 1: GitHub API — Commit Activity + Commits + README

**Files:**
- Create: `src/github/commits.rs`
- Create: `src/github/readme.rs`
- Modify: `src/github/mod.rs`

**Step 1: Create `src/github/commits.rs`**

```rust
use anyhow::Result;
use chrono::{DateTime, Utc};
use super::GitHubClient;

#[derive(Debug, Clone)]
pub struct WeeklyCommitActivity {
    pub week_start: DateTime<Utc>,
    pub total: u32,
    pub days: [u32; 7], // Sun-Sat
}

#[derive(Debug, Clone)]
pub struct CommitInfo {
    pub sha: String,
    pub short_sha: String,
    pub message: String,
    pub author: String,
    pub date: DateTime<Utc>,
    pub parents: Vec<String>,
    pub html_url: String,
}

impl GitHubClient {
    /// Fetch weekly commit activity for the last year (52 weeks).
    /// Uses GitHub's stats API: GET /repos/{owner}/{repo}/stats/commit_activity
    pub async fn fetch_commit_activity(&self, owner: &str, repo: &str) -> Result<Vec<WeeklyCommitActivity>> {
        let result: serde_json::Value = self.octocrab.get(
            format!("/repos/{}/{}/stats/commit_activity", owner, repo),
            None::<&()>,
        ).await?;

        let mut weeks = Vec::new();
        if let Some(arr) = result.as_array() {
            for item in arr {
                let timestamp = item["week"].as_i64().unwrap_or(0);
                let week_start = DateTime::from_timestamp(timestamp, 0)
                    .unwrap_or_default();
                let total = item["total"].as_u64().unwrap_or(0) as u32;
                let mut days = [0u32; 7];
                if let Some(day_arr) = item["days"].as_array() {
                    for (i, d) in day_arr.iter().enumerate().take(7) {
                        days[i] = d.as_u64().unwrap_or(0) as u32;
                    }
                }
                weeks.push(WeeklyCommitActivity { week_start, total, days });
            }
        }
        Ok(weeks)
    }

    /// Fetch recent commits on the default branch (last 50).
    /// Uses GET /repos/{owner}/{repo}/commits
    pub async fn fetch_repo_commits(&self, owner: &str, repo: &str) -> Result<Vec<CommitInfo>> {
        let result: Vec<serde_json::Value> = self.octocrab.get(
            format!("/repos/{}/{}/commits", owner, repo),
            Some(&[("per_page", "50")]),
        ).await?;

        let mut commits = Vec::new();
        for item in &result {
            let sha = item["sha"].as_str().unwrap_or("").to_string();
            let short_sha = sha.chars().take(7).collect();
            let commit = &item["commit"];
            let message = commit["message"].as_str().unwrap_or("")
                .lines().next().unwrap_or("").to_string();
            let author = commit["author"]["name"].as_str().unwrap_or("").to_string();
            let date = commit["author"]["date"].as_str()
                .and_then(|s| s.parse::<DateTime<Utc>>().ok())
                .unwrap_or_default();
            let parents = item["parents"].as_array()
                .map(|arr| arr.iter().filter_map(|p| p["sha"].as_str().map(String::from)).collect())
                .unwrap_or_default();
            let html_url = item["html_url"].as_str().unwrap_or("").to_string();

            commits.push(CommitInfo { sha, short_sha, message, author, date, parents, html_url });
        }
        Ok(commits)
    }
}
```

**Step 2: Create `src/github/readme.rs`**

```rust
use anyhow::Result;
use super::GitHubClient;

impl GitHubClient {
    /// Fetch the README content for a repo. Returns decoded markdown string.
    /// Uses GET /repos/{owner}/{repo}/readme
    pub async fn fetch_readme(&self, owner: &str, repo: &str) -> Result<String> {
        let result: serde_json::Value = self.octocrab.get(
            format!("/repos/{}/{}/readme", owner, repo),
            None::<&()>,
        ).await?;

        let content = result["content"].as_str().unwrap_or("");
        // GitHub returns base64 with newlines embedded
        let cleaned: String = content.chars().filter(|c| !c.is_whitespace()).collect();
        use base64::Engine;
        let bytes = base64::engine::general_purpose::STANDARD.decode(&cleaned)?;
        Ok(String::from_utf8(bytes)?)
    }
}
```

**Step 3: Add `base64` dependency**

Run: `cargo add base64`

**Step 4: Register modules in `src/github/mod.rs`**

Add after `pub mod repos;`:

```rust
pub mod commits;
pub mod readme;
```

**Step 5: Verify it compiles**

Run: `cargo check`
Expected: OK

**Step 6: Commit**

```bash
git add src/github/commits.rs src/github/readme.rs src/github/mod.rs Cargo.toml Cargo.lock
git commit -m "feat: add GitHub API methods for commit activity, commits, and README"
```

---

### Task 2: App State — Extend RepoSection, Add State Fields, Update Messages

**Files:**
- Modify: `src/app.rs`

**Step 1: Write failing tests**

Add to the `tests` module in `src/app.rs`:

```rust
#[test]
fn test_cycle_section_five_way() {
    let mut state = AppState::new();
    state.screen = Screen::RepoDetail {
        repo_full_name: "user/repo".to_string(),
        section: RepoSection::PRs,
    };

    update(&mut state, Message::CycleSection);
    assert!(matches!(state.screen, Screen::RepoDetail { section: RepoSection::Issues, .. }));
    update(&mut state, Message::CycleSection);
    assert!(matches!(state.screen, Screen::RepoDetail { section: RepoSection::CI, .. }));
    update(&mut state, Message::CycleSection);
    assert!(matches!(state.screen, Screen::RepoDetail { section: RepoSection::Commits, .. }));
    update(&mut state, Message::CycleSection);
    assert!(matches!(state.screen, Screen::RepoDetail { section: RepoSection::Info, .. }));
    update(&mut state, Message::CycleSection);
    assert!(matches!(state.screen, Screen::RepoDetail { section: RepoSection::PRs, .. }));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test test_cycle_section_five_way -- --nocapture`
Expected: FAIL — `RepoSection::Commits` doesn't exist

**Step 3: Implement**

3a. Extend `RepoSection` enum (in `src/app.rs`, around line 26):

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepoSection {
    PRs,
    Issues,
    CI,
    Commits,
    Info,
}

impl RepoSection {
    pub fn next(self) -> Self {
        match self {
            Self::PRs => Self::Issues,
            Self::Issues => Self::CI,
            Self::CI => Self::Commits,
            Self::Commits => Self::Info,
            Self::Info => Self::PRs,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::PRs => "PRs",
            Self::Issues => "Issues",
            Self::CI => "CI",
            Self::Commits => "Commits",
            Self::Info => "Info",
        }
    }
}
```

3b. Add imports at top of `src/app.rs`:

```rust
use crate::github::commits::{CommitInfo, WeeklyCommitActivity};
```

3c. Add new state fields to `AppState` (after `pub detail_selected: usize,`):

```rust
    pub repo_commits: Vec<CommitInfo>,
    pub repo_commit_activity: Vec<WeeklyCommitActivity>,
    pub repo_readme: Option<String>,
```

3d. Add defaults in `AppState::new()` (after `detail_selected: 0,`):

```rust
            repo_commits: Vec::new(),
            repo_commit_activity: Vec::new(),
            repo_readme: None,
```

3e. Extend `DetailItem` enum (after `Ci` variant):

```rust
pub enum DetailItem<'a> {
    Pr(&'a PrInfo),
    Issue(&'a IssueInfo),
    Ci(&'a WorkflowRun),
    Commit(&'a CommitInfo),
}
```

3f. Add `Commits` branch to `filtered_detail_items()`:

After the `RepoSection::CI` branch, add:

```rust
            Screen::RepoDetail { section: RepoSection::Commits, .. } => {
                self.repo_commits.iter().map(|c| DetailItem::Commit(c)).collect()
            }
            Screen::RepoDetail { section: RepoSection::Info, .. } => {
                Vec::new() // Info tab doesn't use detail items (it renders README directly)
            }
```

And in the search filter at the bottom, add the `Commit` match arm:

```rust
                DetailItem::Commit(c) => c.message.to_lowercase().contains(&q) || c.author.to_lowercase().contains(&q),
```

3g. Extend `Message::RepoDetailLoaded` to include new fields:

```rust
    RepoDetailLoaded {
        repo: String,
        prs: Vec<PrInfo>,
        issues: Vec<IssueInfo>,
        ci: Vec<WorkflowRun>,
        commits: Vec<CommitInfo>,
        commit_activity: Vec<WeeklyCommitActivity>,
        readme: Option<String>,
    },
```

3h. Update the `RepoDetailLoaded` handler in `update()`:

```rust
        Message::RepoDetailLoaded { repo, prs, issues, ci, commits, commit_activity, readme } => {
            if let Screen::RepoDetail { repo_full_name, .. } = &state.screen {
                if *repo_full_name == repo {
                    state.repo_prs = prs;
                    state.repo_issues = issues;
                    state.repo_ci = ci;
                    state.repo_commits = commits;
                    state.repo_commit_activity = commit_activity;
                    state.repo_readme = readme;
                    state.loading.remove("repo_detail");
                }
            }
        }
```

3i. Update `Message::Select` handler's `Screen::Home` branch — clear new fields when entering repo detail. After `state.repo_ci.clear();` add:

```rust
                                state.repo_commits.clear();
                                state.repo_commit_activity.clear();
                                state.repo_readme = None;
```

3j. Update the existing `test_cycle_section` test to match the new 5-way cycle — it currently expects CI → PRs, but now CI → Commits. Replace the test:

```rust
#[test]
fn test_cycle_section() {
    let mut state = AppState::new();
    state.screen = Screen::RepoDetail {
        repo_full_name: "user/repo".to_string(),
        section: RepoSection::PRs,
    };

    update(&mut state, Message::CycleSection);
    assert!(matches!(state.screen, Screen::RepoDetail { section: RepoSection::Issues, .. }));

    update(&mut state, Message::CycleSection);
    assert!(matches!(state.screen, Screen::RepoDetail { section: RepoSection::CI, .. }));

    update(&mut state, Message::CycleSection);
    assert!(matches!(state.screen, Screen::RepoDetail { section: RepoSection::Commits, .. }));

    update(&mut state, Message::CycleSection);
    assert!(matches!(state.screen, Screen::RepoDetail { section: RepoSection::Info, .. }));

    update(&mut state, Message::CycleSection);
    assert!(matches!(state.screen, Screen::RepoDetail { section: RepoSection::PRs, .. }));
}
```

3k. Update every test that constructs `Message::RepoDetailLoaded` to include the new fields. Grep for `RepoDetailLoaded` in test code — there may be none currently (the existing tests don't use it directly), but if found, add:

```rust
commits: vec![],
commit_activity: vec![],
readme: None,
```

**Step 4: Run tests**

Run: `cargo test -- --nocapture`
Expected: ALL PASS

**Step 5: Commit**

```bash
git add src/app.rs
git commit -m "feat: extend RepoSection to 5 tabs, add commit/readme state fields"
```

---

### Task 3: Wire Parallel Fetching in Main Loop

**Files:**
- Modify: `src/main.rs`

**Step 1: Update the async repo-detail fetch**

In the `KeyCode::Enter` handler (around line 184), replace the existing `tokio::join!` block:

Replace:
```rust
                                            let (prs, issues, ci) = tokio::join!(
                                                client.fetch_repo_prs(owner, name),
                                                client.fetch_repo_issues(owner, name),
                                                client.fetch_repo_ci(owner, name),
                                            );
                                            let _ = tx.send(Message::RepoDetailLoaded {
                                                repo: format!("{}/{}", owner, name),
                                                prs: prs.unwrap_or_default(),
                                                issues: issues.unwrap_or_default(),
                                                ci: ci.unwrap_or_default(),
                                            });
```

With:
```rust
                                            let (prs, issues, ci, commits, activity, readme) = tokio::join!(
                                                client.fetch_repo_prs(owner, name),
                                                client.fetch_repo_issues(owner, name),
                                                client.fetch_repo_ci(owner, name),
                                                client.fetch_repo_commits(owner, name),
                                                client.fetch_commit_activity(owner, name),
                                                client.fetch_readme(owner, name),
                                            );
                                            let _ = tx.send(Message::RepoDetailLoaded {
                                                repo: format!("{}/{}", owner, name),
                                                prs: prs.unwrap_or_default(),
                                                issues: issues.unwrap_or_default(),
                                                ci: ci.unwrap_or_default(),
                                                commits: commits.unwrap_or_default(),
                                                commit_activity: activity.unwrap_or_default(),
                                                readme: readme.ok(),
                                            });
```

**Step 2: Verify it compiles + tests pass**

Run: `cargo test && cargo check`
Expected: ALL PASS

**Step 3: Commit**

```bash
git add src/main.rs
git commit -m "feat: fetch commits, commit activity, and README in parallel on repo enter"
```

---

### Task 4: Render Repo Heatmap (persistent, above tabs)

**Files:**
- Modify: `src/ui/repo_detail.rs`

**Step 1: Update layout to include heatmap area**

In `render()`, change the vertical layout from 3-way to 4-way:

Replace:
```rust
    let [header_area, tabs_area, content_area] = Layout::vertical([
        Constraint::Length(4),
        Constraint::Length(2),
        Constraint::Fill(1),
    ]).areas(area);
```

With:
```rust
    let has_activity = !state.repo_commit_activity.is_empty();
    let heatmap_height = if has_activity { 6 } else { 0 };

    let [header_area, heatmap_area, tabs_area, content_area] = Layout::vertical([
        Constraint::Length(4),
        Constraint::Length(heatmap_height),
        Constraint::Length(2),
        Constraint::Fill(1),
    ]).areas(area);
```

**Step 2: Add heatmap rendering call**

After the repo header rendering and before the tab bar, add:

```rust
    // ── Repo commit heatmap (always visible) ──
    if has_activity {
        render_repo_heatmap(frame, heatmap_area, &state.repo_commit_activity);
    }
```

**Step 3: Implement `render_repo_heatmap`**

Add this function at the bottom of `src/ui/repo_detail.rs`:

```rust
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
```

**Step 4: Verify it compiles**

Run: `cargo check`
Expected: OK

**Step 5: Commit**

```bash
git add src/ui/repo_detail.rs
git commit -m "feat: render persistent repo commit heatmap above tabs"
```

---

### Task 5: Render Commits Tab (branch/merge graph)

**Files:**
- Modify: `src/ui/repo_detail.rs`

**Step 1: Add Commits section to tab bar**

Update the tab bar rendering in `render()`. Replace the existing tabs_line and underline:

```rust
    let commit_count = state.repo_commits.len();
    let info_label = "Info";

    let tabs_line = Line::from(vec![
        Span::raw("  "),
        section_tab("PRs", pr_count, section == RepoSection::PRs),
        Span::raw("    "),
        section_tab("Issues", issue_count, section == RepoSection::Issues),
        Span::raw("    "),
        section_tab("CI", ci_count, section == RepoSection::CI),
        Span::raw("    "),
        section_tab("Commits", commit_count, section == RepoSection::Commits),
        Span::raw("    "),
        section_tab_no_count(info_label, section == RepoSection::Info),
    ]);

    let underline = Line::from(vec![
        Span::raw("  "),
        section_underline(section == RepoSection::PRs),
        Span::raw("    "),
        section_underline(section == RepoSection::Issues),
        Span::raw("    "),
        section_underline(section == RepoSection::CI),
        Span::raw("    "),
        section_underline(section == RepoSection::Commits),
        Span::raw("    "),
        section_underline(section == RepoSection::Info),
    ]);
```

Add helper for tabs without count:

```rust
fn section_tab_no_count(label: &str, active: bool) -> Span<'static> {
    let text = label.to_string();
    if active {
        Span::styled(text, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
    } else {
        Span::styled(text, Style::default().fg(Color::DarkGray))
    }
}
```

**Step 2: Add Commits and Info match arms to the content section**

Update the match in `render()`:

```rust
    match section {
        RepoSection::PRs => render_pr_list(frame, content_area, state),
        RepoSection::Issues => render_issue_list(frame, content_area, state),
        RepoSection::CI => render_ci_list(frame, content_area, state),
        RepoSection::Commits => render_commit_list(frame, content_area, state),
        RepoSection::Info => render_readme(frame, content_area, state),
    }
```

**Step 3: Implement `render_commit_list`**

```rust
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
```

**Step 4: Add placeholder `render_readme` (completed in Task 6)**

```rust
fn render_readme(frame: &mut Frame, area: Rect, state: &AppState) {
    let text = match &state.repo_readme {
        Some(content) => content.as_str(),
        None => "  No README available",
    };
    frame.render_widget(
        Paragraph::new(text).style(Style::default().fg(Color::DarkGray)),
        area,
    );
}
```

**Step 5: Update Select handler for Commits tab**

In `render()`, the `Select` handler in `app.rs` already handles `RepoDetail` by matching on `DetailItem`. Add the Commit arm to the Select handler in `app.rs`'s match on `DetailItem`:

In `src/app.rs`, update the `Select` handler's `Screen::RepoDetail` branch. Find the existing match:

```rust
                            let url = match item {
                                DetailItem::Pr(pr) => &pr.html_url,
                                DetailItem::Issue(i) => &i.html_url,
                                DetailItem::Ci(r) => &r.html_url,
                            };
```

Replace with:

```rust
                            let url = match item {
                                DetailItem::Pr(pr) => &pr.html_url,
                                DetailItem::Issue(i) => &i.html_url,
                                DetailItem::Ci(r) => &r.html_url,
                                DetailItem::Commit(c) => &c.html_url,
                            };
```

**Step 6: Verify it compiles**

Run: `cargo check`
Expected: OK

**Step 7: Commit**

```bash
git add src/ui/repo_detail.rs src/app.rs
git commit -m "feat: render commits tab with branch/merge graph visualization"
```

---

### Task 6: Render Info Tab (README with styled markdown)

**Files:**
- Modify: `src/ui/repo_detail.rs`

**Step 1: Replace the placeholder `render_readme` with full implementation**

```rust
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
            if in_code_block {
                lines.push(Line::from(Span::styled(
                    format!("  {}", raw_line),
                    Style::default().fg(Color::DarkGray),
                )));
            } else {
                lines.push(Line::from(Span::styled(
                    format!("  {}", raw_line),
                    Style::default().fg(Color::DarkGray),
                )));
            }
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
        if trimmed.starts_with("### ") {
            lines.push(Line::from(Span::styled(
                format!("  {}", &trimmed[4..]),
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
            )));
        } else if trimmed.starts_with("## ") {
            lines.push(Line::from(Span::styled(
                format!("  {}", &trimmed[3..]),
                Style::default().fg(Color::Cyan),
            )));
        } else if trimmed.starts_with("# ") {
            lines.push(Line::from(Span::styled(
                format!("  {}", &trimmed[2..]),
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            )));
        }
        // List items
        else if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled("• ", Style::default().fg(Color::Cyan)),
                Span::raw(&trimmed[2..]),
            ]));
        }
        // Blank lines
        else if trimmed.is_empty() {
            lines.push(Line::from(""));
        }
        // Regular text — parse inline formatting
        else {
            lines.push(Line::from(parse_inline_markdown(trimmed)));
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
    let mut spans: Vec<Span> = vec![Span::raw("  ")]; // left padding
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
                while let Some(c) = chars.next() {
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
                while let Some(c) = chars.next() {
                    if c == ']' { break; }
                    link_text.push(c);
                }
                // Skip the (url) part
                if chars.peek() == Some(&'(') {
                    chars.next();
                    while let Some(c) = chars.next() {
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
```

**Step 2: Update Up/Down navigation for Info tab scroll**

In `src/app.rs`, the Info tab uses `detail_selected` as a scroll offset (line-by-line) rather than an item index. The existing Up/Down handlers already increment/decrement `detail_selected` — this works naturally. However, the bounds check uses `filtered_detail_items().len()` which returns 0 for Info. We need to handle this.

In the `Message::Down` handler's `Screen::RepoDetail` branch, replace:

```rust
                    Screen::RepoDetail { .. } => {
                        let len = state.filtered_detail_items().len();
                        if state.detail_selected < len.saturating_sub(1) {
                            state.detail_selected += 1;
                        }
                    }
```

With:

```rust
                    Screen::RepoDetail { section, .. } => {
                        if *section == RepoSection::Info {
                            // Info tab: scroll README line by line (no upper bound — just increment)
                            state.detail_selected += 1;
                        } else {
                            let len = state.filtered_detail_items().len();
                            if state.detail_selected < len.saturating_sub(1) {
                                state.detail_selected += 1;
                            }
                        }
                    }
```

Note: The `section` is already matched from the enum — we need to destructure it. Since the `Down` handler already matches `Screen::RepoDetail { .. }`, change to `Screen::RepoDetail { section, .. }` so we can access `section`.

**Step 3: Verify it compiles + tests pass**

Run: `cargo test && cargo check`
Expected: ALL PASS

**Step 4: Commit**

```bash
git add src/ui/repo_detail.rs src/app.rs
git commit -m "feat: render README with styled markdown in Info tab"
```

---

### Task 7: Final Polish — Verify Everything Works Together

**Step 1: Full test suite**

Run: `cargo test -- --nocapture`
Expected: ALL PASS

**Step 2: Clippy**

Run: `cargo clippy`
Expected: No errors (pre-existing warnings OK)

**Step 3: Manual smoke test**

Run: `cargo run`

Verify:
- [ ] Navigate to a repo (Enter)
- [ ] Heatmap shows above tabs with commit activity
- [ ] Tab cycles through: PRs → Issues → CI → Commits → Info
- [ ] Commits tab shows graph with short SHA, message, author, time
- [ ] Merge commits show `|\` connector
- [ ] Enter on a commit opens it in browser
- [ ] Info tab shows README with styled headers, code blocks, bold, links
- [ ] j/k scrolls README content
- [ ] Search (/) filters commits by message/author
- [ ] Esc goes back to home
- [ ] Heatmap stays visible on every tab

**Step 4: Commit any fixes if needed**

```bash
git add -A
git commit -m "fix: polish repo detail enhancements"
```
