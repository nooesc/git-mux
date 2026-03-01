# GHD Repo Detail Enhancements Design

## Goal

Add three features to the repo detail page: (1) a persistent repo-scoped commit heatmap at the top, (2) a Commits tab with branch/merge graph visualization, and (3) an Info tab that renders the repo's README as styled markdown.

## Architecture

The heatmap is always visible above the tab bar — it doesn't change when switching sections. Two new sections (Commits, Info) extend `RepoSection` from 3 to 5 variants. All new data (commit stats, commits list, README) is fetched in parallel alongside the existing PRs/Issues/CI when entering a repo.

## Feature 1: Repo Commit Heatmap (persistent, top of detail page)

**What:** A compact heatmap showing commit frequency for this specific repo, always visible above the tab bar regardless of which section is active. Uses the same half-block visual style as the home screen contribution graph but scoped to the repo.

**Layout change:**

```
┌─ owner/repo ────────────────────────────────┐
│ ★ 13 · Rust · 2 forks · 1d ago             │
│     Jan     Feb     Mar                      │
│ Mon ░▒░░░░▓░░░░░▒░░░░░░░▓▓░░░░░░░░░        │
│ Wed ░░▒░░░░░░░▒░░░░░░▒░░░░░▒░░░░░░░        │
│ Fri ░░░░░▓░░░░░░░░░▓░░░░░░░░░░▓░░░░        │
│  142 commits this year                       │
│  PRs (5)    Issues (3)    CI (8)    Commits    Info  │
│  ────────                                    │
│  > #42  Fix login redirect        2h ago     │
│    #41  Add user settings         5h ago     │
└─────────────────────────────────────────────┘
```

**API:** `GET /repos/{owner}/{repo}/stats/commit_activity` — returns 52 weeks of commit count data. Single lightweight call.

**Data struct:**

```rust
pub struct WeeklyCommitActivity {
    pub week_start: DateTime<Utc>,
    pub total: u32,
    pub days: [u32; 7],  // Sun-Sat
}
```

**State:** `repo_commit_activity: Vec<WeeklyCommitActivity>` on AppState, cleared when leaving repo detail.

**Rendering:** Reuse the same `level_to_cell` color mapping from `ui/home.rs`. Compute levels from commit counts (0 = none, scale 1-4 based on max). Render day labels + week columns. Stats line below.

## Feature 2: Commits Tab (branch/merge graph)

**What:** A new `Commits` section in the tab bar showing recent commits with ASCII graph visualization.

**API:** `GET /repos/{owner}/{repo}/commits` — fetches last 50 commits on default branch with author, message, SHA, date, and parent SHAs.

**Data struct:**

```rust
pub struct CommitInfo {
    pub sha: String,       // full SHA
    pub short_sha: String, // first 7 chars
    pub message: String,   // first line only
    pub author: String,
    pub date: DateTime<Utc>,
    pub parents: Vec<String>,  // parent SHAs (2+ = merge)
    pub html_url: String,
}
```

**State:** `repo_commits: Vec<CommitInfo>` on AppState.

**Rendering:** Each commit takes 2 lines:
- Line 1: `* <short_sha> <message>` (merge commits show `*   <sha> Merge ...`)
- Line 2: `|  <author> · <time_ago>`

Merge commits detected by `parents.len() > 1`. Graph lines (`|`, `\`, `/`) drawn for simple cases — single-branch linear history gets `*` and `|`, merges get `|\` and `|/` connectors.

**Navigation:** j/k scrolls, Enter opens commit in browser.

## Feature 3: Info Tab (rendered README)

**What:** A new `Info` section that fetches and renders the repo's README as styled markdown.

**API:** `GET /repos/{owner}/{repo}/readme` — returns base64-encoded README content. Decode to string.

**State:** `repo_readme: Option<String>` on AppState (raw markdown text).

**Rendering:** Parse markdown and render with ratatui styles:
- `# H1`: cyan + bold, `## H2`: cyan, `### H3`: white + bold
- `**bold**`: bold modifier
- `*italic*`: italic modifier
- `` `inline code` ``: dark gray background
- Code blocks (```): dimmed foreground, indented
- `- list items`: indented with `•` bullet
- `[links](url)`: underlined cyan
- Blank lines preserved

No external markdown crate needed — simple line-by-line parser handles common patterns. Scrollable with j/k.

## RepoSection Changes

```rust
pub enum RepoSection {
    PRs,
    Issues,
    CI,
    Commits,
    Info,
}
```

`CycleSection` (Tab key) rotates: PRs → Issues → CI → Commits → Info → PRs.

## Message Changes

Extend `RepoDetailLoaded` to include new data:

```rust
Message::RepoDetailLoaded {
    repo,
    prs,
    issues,
    ci,
    commits,
    commit_activity,
    readme,
}
```

All 6 fetches happen in parallel with `tokio::join!`.

## Files Modified

| File | Changes |
|------|---------|
| `src/github/commits.rs` | New file: `CommitInfo`, `WeeklyCommitActivity` structs, `fetch_repo_commits()`, `fetch_commit_activity()` methods |
| `src/github/readme.rs` | New file: `fetch_readme()` method |
| `src/github/mod.rs` | Add `pub mod commits;` and `pub mod readme;` |
| `src/app.rs` | Extend `RepoSection` (add Commits, Info), add state fields, update `RepoDetailLoaded` handler, update `filtered_detail_items()`, update `CycleSection` |
| `src/ui/repo_detail.rs` | Add heatmap rendering at top (always visible), add Commits section renderer, add Info/README renderer, update layout, update tab bar |
| `src/main.rs` | Update async fetch to include commits, commit_activity, readme in parallel |

## Key Bindings

No new keys. Tab cycles through 5 sections instead of 3. j/k scrolls. Enter opens in browser (commits → commit URL, info tab → no-op or repo URL).
