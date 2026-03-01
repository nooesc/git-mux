# GHD Filter Bar, Selection Highlight & Config Exclusions Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add background fill on selected repos, an inline filter bar (All/Public/Private/per-org), and config-driven exclusion of orgs and repos.

**Architecture:** Config exclusions filter at data-load time in `update()`. Filter bar adds a `HomeFocus` concept to home screen navigation — j/k moves between filter bar and repos, h/l cycles filters. Background fill is a simple style change on selected cards/rows.

**Tech Stack:** Rust, ratatui 0.29, serde/toml for config

---

### Task 1: Config Struct — Add `[repos]` Section

**Files:**
- Modify: `src/config.rs`

**Step 1: Write the failing test**

Add to the `tests` module in `src/config.rs`:

```rust
#[test]
fn test_repo_exclusion_config() {
    let config: Config = toml::from_str(r#"
        [repos]
        exclude = ["owner/repo-name", "org/internal-tool"]
    "#).unwrap();
    assert_eq!(config.repos.exclude.len(), 2);
    assert_eq!(config.repos.exclude[0], "owner/repo-name");
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test test_repo_exclusion_config -- --nocapture`
Expected: FAIL — `Config` has no `repos` field

**Step 3: Implement**

Add the `RepoConfig` struct and wire it into `Config`. In `src/config.rs`:

Add after `OrgConfig`:

```rust
#[derive(Debug, Deserialize, Serialize, Default, Clone)]
pub struct RepoConfig {
    #[serde(default)]
    pub exclude: Vec<String>,
}
```

Add `Clone` derive to `OrgConfig`:

```rust
#[derive(Debug, Deserialize, Serialize, Default, Clone)]
pub struct OrgConfig {
```

Add `Clone` derive to `GeneralConfig`:

```rust
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct GeneralConfig {
```

Add `Clone` derive to `Config`:

```rust
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Config {
```

Add field to `Config` struct:

```rust
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Config {
    #[serde(default = "default_general")]
    pub general: GeneralConfig,
    #[serde(default)]
    pub orgs: OrgConfig,
    #[serde(default)]
    pub repos: RepoConfig,
}
```

Update the default config construction in `Config::load()` (the `else` branch):

```rust
let config = Config {
    general: default_general(),
    orgs: OrgConfig::default(),
    repos: RepoConfig::default(),
};
```

**Step 4: Run tests**

Run: `cargo test -- --nocapture`
Expected: ALL PASS (including new test + existing `test_default_config` and `test_partial_config`)

**Step 5: Commit**

```bash
git add src/config.rs
git commit -m "feat: add [repos] exclude section to config"
```

---

### Task 2: App State — Add HomeFocus, RepoFilter, Config Storage

**Files:**
- Modify: `src/app.rs`

**Step 1: Write the failing tests**

Add these tests to the `tests` module in `src/app.rs`:

```rust
#[test]
fn test_initial_home_focus() {
    let state = AppState::new();
    assert_eq!(state.home_focus, HomeFocus::Repos);
    assert_eq!(state.repo_filter, RepoFilter::All);
    assert_eq!(state.filter_index, 0);
}

#[test]
fn test_filter_options_generation() {
    let mut state = AppState::new();
    state.repos = vec![
        make_repo("alice/foo"),
        make_repo_private("alice/secret"),
        make_repo("acme-corp/tool"),
    ];
    state.user_info = Some(crate::github::UserInfo {
        login: "alice".to_string(),
        avatar_url: String::new(),
        public_repos: 2,
        followers: 0,
    });
    let opts = state.filter_options();
    // All, Public, Private, acme-corp
    assert_eq!(opts.len(), 4);
    assert_eq!(opts[0], RepoFilter::All);
    assert_eq!(opts[1], RepoFilter::Public);
    assert_eq!(opts[2], RepoFilter::Private);
    assert_eq!(opts[3], RepoFilter::Org("acme-corp".to_string()));
}

#[test]
fn test_filtered_repos_by_filter() {
    let mut state = AppState::new();
    state.repos = vec![
        make_repo("alice/public1"),
        make_repo_private("alice/secret"),
        make_repo("acme-corp/tool"),
    ];

    state.repo_filter = RepoFilter::All;
    assert_eq!(state.filtered_repos().len(), 3);

    state.repo_filter = RepoFilter::Public;
    assert_eq!(state.filtered_repos().len(), 2); // public1 + tool

    state.repo_filter = RepoFilter::Private;
    assert_eq!(state.filtered_repos().len(), 1); // secret

    state.repo_filter = RepoFilter::Org("acme-corp".to_string());
    assert_eq!(state.filtered_repos().len(), 1); // tool
}
```

Also add the `make_repo_private` test helper:

```rust
fn make_repo_private(name: &str) -> RepoInfo {
    let mut repo = make_repo(name);
    repo.is_private = true;
    repo
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test test_initial_home_focus test_filter_options_generation test_filtered_repos_by_filter -- --nocapture`
Expected: FAIL — `HomeFocus` and `RepoFilter` don't exist

**Step 3: Implement**

Add after the `ViewMode` enum in `src/app.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HomeFocus {
    FilterBar,
    Repos,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepoFilter {
    All,
    Public,
    Private,
    Org(String),
}
```

Add fields to `AppState` struct (after the `home_scroll` line):

```rust
    pub home_focus: HomeFocus,
    pub repo_filter: RepoFilter,
    pub filter_index: usize,

    // Config (for exclusions)
    pub exclude_orgs: Vec<String>,
    pub exclude_repos: Vec<String>,
```

Add defaults in `AppState::new()` (after `home_scroll: 0,`):

```rust
            home_focus: HomeFocus::Repos,
            repo_filter: RepoFilter::All,
            filter_index: 0,
            exclude_orgs: Vec::new(),
            exclude_repos: Vec::new(),
```

Add `filter_options()` method to `impl AppState` (after `num_card_cols`):

```rust
    /// Build the list of filter options from loaded repos.
    pub fn filter_options(&self) -> Vec<RepoFilter> {
        let mut opts = vec![RepoFilter::All, RepoFilter::Public, RepoFilter::Private];
        let username = self.user_info.as_ref().map(|u| u.login.as_str()).unwrap_or("");
        let mut orgs: Vec<&str> = self.repos.iter()
            .map(|r| r.owner.as_str())
            .filter(|o| !o.is_empty() && *o != username)
            .collect();
        orgs.sort_unstable();
        orgs.dedup();
        for org in orgs {
            opts.push(RepoFilter::Org(org.to_string()));
        }
        opts
    }
```

Update `filtered_repos()` to apply `repo_filter`:

```rust
    /// Get repos filtered by search query and repo filter.
    pub fn filtered_repos(&self) -> Vec<&RepoInfo> {
        let filter_iter = self.repos.iter().filter(|r| {
            match &self.repo_filter {
                RepoFilter::All => true,
                RepoFilter::Public => !r.is_private,
                RepoFilter::Private => r.is_private,
                RepoFilter::Org(name) => r.owner == *name,
            }
        });

        if self.search_query.is_empty() {
            return filter_iter.collect();
        }
        let q = self.search_query.to_lowercase();
        filter_iter.filter(|r| {
            r.full_name.to_lowercase().contains(&q)
                || r.description.as_ref().is_some_and(|d| d.to_lowercase().contains(&q))
                || r.language.as_ref().is_some_and(|l| l.to_lowercase().contains(&q))
        }).collect()
    }
```

**Step 4: Run tests**

Run: `cargo test -- --nocapture`
Expected: ALL PASS

**Step 5: Commit**

```bash
git add src/app.rs
git commit -m "feat: add HomeFocus, RepoFilter enums and filter logic"
```

---

### Task 3: Config Exclusions — Filter on ReposLoaded

**Files:**
- Modify: `src/app.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn test_config_exclusions() {
    let mut state = AppState::new();
    state.exclude_orgs = vec!["boring-corp".to_string()];
    state.exclude_repos = vec!["alice/junk".to_string()];
    state.loading.insert("repos".to_string());

    let repos = vec![
        make_repo("alice/good"),
        make_repo("alice/junk"),
        make_repo("boring-corp/tool"),
        make_repo("acme/nice"),
    ];
    update(&mut state, Message::ReposLoaded(repos));

    assert_eq!(state.repos.len(), 2);
    assert_eq!(state.repos[0].full_name, "alice/good");
    assert_eq!(state.repos[1].full_name, "acme/nice");
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test test_config_exclusions -- --nocapture`
Expected: FAIL — `state.repos.len()` is 4 (no exclusions applied)

**Step 3: Implement**

Update `Message::ReposLoaded` handler in `update()`:

```rust
        Message::ReposLoaded(repos) => {
            state.repos = repos.into_iter().filter(|r| {
                !state.exclude_orgs.iter().any(|o| o.eq_ignore_ascii_case(&r.owner))
                    && !state.exclude_repos.iter().any(|e| e.eq_ignore_ascii_case(&r.full_name))
            }).collect();
            state.loading.remove("repos");
        }
```

**Step 4: Run tests**

Run: `cargo test -- --nocapture`
Expected: ALL PASS

**Step 5: Commit**

```bash
git add src/app.rs
git commit -m "feat: apply config exclusions when repos load"
```

---

### Task 4: Navigation — HomeFocus Awareness in Update

**Files:**
- Modify: `src/app.rs`

**Step 1: Write the failing tests**

```rust
#[test]
fn test_up_from_first_repo_moves_to_filter_bar() {
    let mut state = AppState::new();
    state.home_focus = HomeFocus::Repos;
    state.card_selected = 0;
    state.view_mode = ViewMode::List;
    state.repos = vec![make_repo("a/b")];

    update(&mut state, Message::Up);
    assert_eq!(state.home_focus, HomeFocus::FilterBar);
}

#[test]
fn test_down_from_filter_bar_moves_to_repos() {
    let mut state = AppState::new();
    state.home_focus = HomeFocus::FilterBar;
    state.repos = vec![make_repo("a/b")];

    update(&mut state, Message::Down);
    assert_eq!(state.home_focus, HomeFocus::Repos);
}

#[test]
fn test_left_right_on_filter_bar() {
    let mut state = AppState::new();
    state.home_focus = HomeFocus::FilterBar;
    state.filter_index = 0;
    state.repos = vec![make_repo("alice/foo"), make_repo("acme/bar")];
    state.user_info = Some(crate::github::UserInfo {
        login: "alice".to_string(),
        avatar_url: String::new(),
        public_repos: 1,
        followers: 0,
    });
    // filter_options: All, Public, Private, acme
    update(&mut state, Message::Right);
    assert_eq!(state.filter_index, 1);
    assert_eq!(state.repo_filter, RepoFilter::Public);

    update(&mut state, Message::Right);
    assert_eq!(state.filter_index, 2);
    assert_eq!(state.repo_filter, RepoFilter::Private);

    update(&mut state, Message::Left);
    assert_eq!(state.filter_index, 1);
    assert_eq!(state.repo_filter, RepoFilter::Public);
}

#[test]
fn test_select_on_filter_bar_drops_to_repos() {
    let mut state = AppState::new();
    state.home_focus = HomeFocus::FilterBar;
    state.repos = vec![make_repo("a/b")];

    update(&mut state, Message::Select);
    assert_eq!(state.home_focus, HomeFocus::Repos);
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test test_up_from_first_repo test_down_from_filter_bar test_left_right_on_filter_bar test_select_on_filter_bar -- --nocapture`
Expected: FAIL

**Step 3: Implement**

Replace the `Message::Up` handler's `Screen::Home` branch:

```rust
                    Screen::Home => {
                        if state.home_focus == HomeFocus::FilterBar {
                            // Already at top, do nothing
                        } else {
                            let at_top = match state.view_mode {
                                ViewMode::Cards => state.card_selected < state.num_card_cols(),
                                ViewMode::List => state.card_selected == 0,
                            };
                            if at_top {
                                state.home_focus = HomeFocus::FilterBar;
                            } else {
                                match state.view_mode {
                                    ViewMode::Cards => {
                                        let cols = state.num_card_cols();
                                        if state.card_selected >= cols {
                                            state.card_selected -= cols;
                                        }
                                    }
                                    ViewMode::List => {
                                        if state.card_selected > 0 {
                                            state.card_selected -= 1;
                                        }
                                    }
                                }
                            }
                        }
                    }
```

Replace the `Message::Down` handler's `Screen::Home` branch:

```rust
                    Screen::Home => {
                        if state.home_focus == HomeFocus::FilterBar {
                            state.home_focus = HomeFocus::Repos;
                        } else {
                            let cols = state.num_card_cols();
                            let len = state.filtered_repos().len();
                            match state.view_mode {
                                ViewMode::Cards => {
                                    if state.card_selected + cols < len {
                                        state.card_selected += cols;
                                    }
                                }
                                ViewMode::List => {
                                    if state.card_selected < len.saturating_sub(1) {
                                        state.card_selected += 1;
                                    }
                                }
                            }
                        }
                    }
```

Replace the `Message::Left` handler:

```rust
        Message::Left => {
            if state.screen == Screen::Home {
                if state.home_focus == HomeFocus::FilterBar {
                    if state.filter_index > 0 {
                        state.filter_index -= 1;
                        let opts = state.filter_options();
                        state.repo_filter = opts[state.filter_index].clone();
                        state.card_selected = 0;
                    }
                } else if state.view_mode == ViewMode::Cards && state.card_selected > 0 {
                    state.card_selected -= 1;
                }
            }
        }
```

Replace the `Message::Right` handler:

```rust
        Message::Right => {
            if state.screen == Screen::Home {
                if state.home_focus == HomeFocus::FilterBar {
                    let opts = state.filter_options();
                    if state.filter_index + 1 < opts.len() {
                        state.filter_index += 1;
                        state.repo_filter = opts[state.filter_index].clone();
                        state.card_selected = 0;
                    }
                } else if state.view_mode == ViewMode::Cards {
                    let len = state.filtered_repos().len();
                    if state.card_selected + 1 < len {
                        state.card_selected += 1;
                    }
                }
            }
        }
```

Update `Message::Select` handler — add filter bar case at the top of the `Screen::Home` branch:

```rust
                    Screen::Home => {
                        if state.home_focus == HomeFocus::FilterBar {
                            state.home_focus = HomeFocus::Repos;
                        } else {
                            // Drill into repo detail (existing code)
                            let filtered = state.filtered_repos();
                            if let Some(repo) = filtered.get(state.card_selected) {
                                let repo_full_name = repo.full_name.clone();
                                state.screen = Screen::RepoDetail {
                                    repo_full_name,
                                    section: RepoSection::PRs,
                                };
                                state.detail_selected = 0;
                                state.repo_prs.clear();
                                state.repo_issues.clear();
                                state.repo_ci.clear();
                                state.loading.insert("repo_detail".to_string());
                                state.search_query.clear();
                                state.search_mode = false;
                            }
                        }
                    }
```

**Step 4: Run tests**

Run: `cargo test -- --nocapture`
Expected: ALL PASS

**Step 5: Commit**

```bash
git add src/app.rs
git commit -m "feat: HomeFocus-aware navigation for filter bar"
```

---

### Task 5: Selection Background Fill

**Files:**
- Modify: `src/ui/home.rs`

**Step 1: Implement card background fill**

In `render_card()`, change the block construction to include a background when selected:

```rust
fn render_card(frame: &mut Frame, area: Rect, repo: &RepoInfo, selected: bool) {
    let (border_style, bg) = if selected {
        (Style::default().fg(Color::Cyan), Color::Rgb(30, 40, 50))
    } else {
        (Style::default().fg(Color::DarkGray), Color::Reset)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .style(Style::default().bg(bg));
    let inner = block.inner(area);
    frame.render_widget(block, area);
```

**Step 2: Implement list view background fill**

In `render_list_view()`, update the selected row styling. Replace the style and line construction for each repo:

```rust
            let (style, row_bg) = if *flat_idx == selected {
                (Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD), Color::Rgb(30, 40, 50))
            } else {
                (Style::default(), Color::Reset)
            };

            let mut line = Line::from(vec![
                Span::styled(prefix, style),
                Span::raw(visibility),
                Span::styled(format!(" {:<20}", name), style),
                Span::styled(format!("{:<8}", lang), Style::default().fg(Color::Magenta).bg(row_bg)),
                Span::styled(format!("{:<6}", stars), Style::default().fg(Color::Yellow).bg(row_bg)),
                Span::styled(format!("{:<5}", forks), Style::default().fg(Color::DarkGray).bg(row_bg)),
                Span::styled(pushed, Style::default().fg(Color::DarkGray).bg(row_bg)),
            ]);
            line.patch_style(Style::default().bg(row_bg));
            lines.push(line);
```

Remove the old `let style = ...` and `lines.push(Line::from(vec![...]))` block.

**Step 3: Verify it compiles**

Run: `cargo check`
Expected: OK

**Step 4: Commit**

```bash
git add src/ui/home.rs
git commit -m "feat: add background fill on selected card/row"
```

---

### Task 6: Filter Bar Rendering

**Files:**
- Modify: `src/ui/home.rs`

**Step 1: Add HomeFocus import**

Add `HomeFocus` to the import at the top of `src/ui/home.rs`:

```rust
use crate::app::{AppState, HomeFocus, RepoFilter, ViewMode};
```

**Step 2: Update layout to include filter bar row**

In the `render()` function, change the vertical layout from 2-way to 3-way split:

```rust
pub fn render(frame: &mut Frame, area: Rect, state: &AppState) {
    let show_avatar = state.term_width >= 80;
    let profile_height: u16 = if show_avatar { 14 } else { 10 };

    let [top_area, filter_area, cards_area] = Layout::vertical([
        Constraint::Length(profile_height),
        Constraint::Length(1),
        Constraint::Fill(1),
    ]).areas(area);

    if show_avatar {
        render_profile_and_graph(frame, top_area, state);
    } else {
        render_heatmap(frame, top_area, &state.contributions.days, state.contributions.total);
    }

    render_filter_bar(frame, filter_area, state);

    let filtered = state.filtered_repos();
    match state.view_mode {
        ViewMode::Cards => render_card_grid(frame, cards_area, &filtered, state.card_selected, state.num_card_cols()),
        ViewMode::List => render_list_view(frame, cards_area, &filtered, state.card_selected),
    }
}
```

**Step 3: Implement `render_filter_bar`**

Add this function after `render_profile_and_graph`:

```rust
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
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
        } else if is_active {
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
        } else if is_cursor {
            Style::default().fg(Color::White).add_modifier(Modifier::UNDERLINED)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        spans.push(Span::styled(format!(" {} ", label), style));

        if i + 1 < opts.len() {
            spans.push(Span::styled("│", Style::default().fg(Color::DarkGray)));
        }
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}
```

**Step 4: Verify it compiles**

Run: `cargo check`
Expected: OK

**Step 5: Commit**

```bash
git add src/ui/home.rs
git commit -m "feat: render inline filter bar with active/cursor states"
```

---

### Task 7: Wire Config Into Main + Final Integration

**Files:**
- Modify: `src/main.rs`

**Step 1: Pass config exclusions into AppState**

In `run()`, replace:

```rust
    let _config = config::Config::load()?;

    let mut state = AppState::new();
```

with:

```rust
    let config = config::Config::load()?;

    let mut state = AppState::new();
    state.exclude_orgs = config.orgs.exclude.clone();
    state.exclude_repos = config.repos.exclude.clone();
```

**Step 2: Guard Enter key against filter bar focus**

In the `KeyCode::Enter` handler in `main.rs`, the async repo-detail fetch should only fire when on the repos grid, not the filter bar. Update the condition:

Replace:
```rust
                        KeyCode::Enter => {
                            if state.screen == Screen::Home
                                && let Some(repo) = state.selected_repo() {
```

with:
```rust
                        KeyCode::Enter => {
                            if state.screen == Screen::Home
                                && state.home_focus == app::HomeFocus::Repos
                                && let Some(repo) = state.selected_repo() {
```

You'll need to add `app::HomeFocus` to scope — it's already accessible via the `app` module import.

**Step 3: Verify full build + tests**

Run: `cargo test && cargo clippy`
Expected: ALL PASS, no errors

**Step 4: Commit**

```bash
git add src/main.rs
git commit -m "feat: wire config exclusions and filter bar guard into main loop"
```

---

### Task 8: Final Polish — Verify Everything Works Together

**Step 1: Full test suite**

Run: `cargo test -- --nocapture`
Expected: ALL PASS (should be ~30+ tests now)

**Step 2: Clippy**

Run: `cargo clippy`
Expected: No errors (warnings for pre-existing dead code are OK)

**Step 3: Manual smoke test**

Run: `cargo run`

Verify:
- [ ] Selected card has subtle blue-gray background fill
- [ ] Selected list row has background fill
- [ ] Filter bar shows between profile section and repos
- [ ] h/l on filter bar cycles between All/Public/Private/orgs
- [ ] k from first repo moves focus to filter bar (underline visible)
- [ ] j from filter bar drops back to repos
- [ ] Enter on filter bar drops to repos
- [ ] Repos update immediately when filter changes
- [ ] Config exclusions work (add an exclusion to `~/.config/ghd/config.toml` and restart)

**Step 4: Commit any fixes if needed**

```bash
git add -A
git commit -m "fix: polish filter bar and selection highlight"
```
