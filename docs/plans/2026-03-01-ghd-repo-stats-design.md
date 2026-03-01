# Repo Stats — Bordered Dashboard Panels Design

## Goal

Replace the flat header + heatmap area with four bordered panels that display richer repository statistics without adding any vertical lines.

## Architecture

The current 10-line top area (4-line header + 6-line heatmap) becomes two rows of side-by-side bordered `Block` widgets. Each row is 5 lines (1 top border + 3 content + 1 bottom border). Layout uses `Layout::horizontal` to split each row ~55/45.

**Tech:** Rust, ratatui `Block` with `Borders::ALL`, 3 new GitHub REST API calls added to the existing `tokio::join!` in main.rs.

---

## Layout

```
┌─ Repo ──────────────────────────────────┐ ┌─ Health ──────────────────────────────┐
│ Influenceable-io/influenceable-app 🔒   │ │ Contributors: 5                      │
│ Influenceable Internal command cen...   │ │  nooesc (42)  alen71 (18)  george (8)│
│ TypeScript · ★ 1 · ⎚ 0 · ⚠ 7 · 2d ago │ │ Avg merge: 2.3d  Close rate: 78%     │
└─────────────────────────────────────────┘ └──────────────────────────────────────┘
┌─ Activity · 204 commits · +2.4k / -1.1k ┐ ┌─ Languages ──────────────────────────┐
│ Mon ░░▒▓░░░░▒▓▓░░▒▒░░                   │ │ ████████████████░░░░ TypeScript  78% │
│ Wed ░░░▒░░░░▒▒░░░░░░░░                   │ │ ████░░░░░░░░░░░░░░░ CSS          18% │
│ Fri ░░▒░░░░░░▒▒░▓░░░░░                   │ │ █░░░░░░░░░░░░░░░░░░ Other         4% │
└──────────────────────────────────────────┘ └──────────────────────────────────────┘
 PRs (76)  Issues (53)  CI (20)  Commits (50)  Info
 ━━━━━━━━  ─────────── ──────── ───────────── ────
```

### Row 1: Repo + Health

**Repo box (3 inner lines):**
1. `{full_name}  🔒 Private` (or `Public`)
2. Description (truncated to fit box width)
3. `{language} · ★ {stars} · ⎚ {forks} · ⚠ {issues} · pushed {ago}`

**Health box (3 inner lines):**
1. `Contributors: {N}`
2. Top 3 contributors: `{login} ({commits})` spaced evenly
3. `Avg merge: {X}d  Close rate: {Y}%`

### Row 2: Activity + Languages

**Activity box (title includes stats):**
- Title: `Activity · {total} commits · +{add} / -{del}`
  - `+{add}` in green, `-{del}` in red
  - Sums from last 4 weeks of code_frequency API
- Content: 3 heatmap rows (Mon/Wed/Fri) — same rendering as current

**Languages box (3 inner lines):**
- Top 3 languages by bytes
- Each line: proportional bar (20 chars) + language name + percentage
- Bar uses `█` for filled, `░` for empty
- Colors: primary language cyan, others white

---

## New GitHub API Calls

Added to the `tokio::join!` when entering repo detail:

| Endpoint | Data | Retry? |
|----------|------|--------|
| `GET /repos/{o}/{r}/languages` | `{"TypeScript": 123456, "CSS": 23456}` | No |
| `GET /repos/{o}/{r}/stats/contributors` | Per-author weekly commit breakdowns | Yes (202 pattern) |
| `GET /repos/{o}/{r}/stats/code_frequency` | Weekly `[timestamp, additions, deletions]` | Yes (202 pattern) |

Total API calls per repo detail: 6 existing + 3 new = 9 parallel calls.

---

## Derived Stats (No API)

- **Avg merge time:** From `repo_prs` where `merged == true`, compute average of `updated_at - created_at` in days. Show `--` if no merged PRs.
- **Close rate:** From `repo_issues`, `(closed / total) * 100`. Only counts issues that have a closed state.

---

## Data Model

New fields in `AppState`:

```rust
pub repo_languages: Vec<(String, u64)>,           // (language, bytes) sorted desc
pub repo_contributors: Vec<ContributorInfo>,       // sorted by commits desc
pub repo_code_frequency: Vec<(i64, i64, i64)>,     // (week_ts, additions, deletions)
```

New struct in `src/github/`:

```rust
pub struct ContributorInfo {
    pub login: String,
    pub total_commits: u32,
}
```

New message variant:

```rust
Message::RepoDetailLoaded {
    // existing fields...
    languages: Vec<(String, u64)>,
    contributors: Vec<ContributorInfo>,
    code_frequency: Vec<(i64, i64, i64)>,
}
```

---

## Rendering Details

### Border Style
- Borders: `Borders::ALL`, `border_style: Style::default().fg(Color::DarkGray)`
- Title: `Style::default().fg(Color::Cyan)`
- Gap between left/right boxes: 1 char (from Layout spacing)

### Health Box Colors
- `Contributors:` label in white, count in cyan
- Contributor logins in magenta, commit counts in dark gray
- `Avg merge:` value in yellow, `Close rate:` value in green (>50%) or red (<50%)

### Language Bar Colors
- Bar `█` segments in green (matching existing heatmap aesthetic)
- Bar `░` background in dark gray
- Language name in white, percentage in dark gray

### Fallback States
- If languages API returns empty: show `No language data` in Languages box
- If contributors API returns empty/202: show `Loading...` then `No data`
- If no merged PRs: show `Avg merge: --`
- If no issues: show `Close rate: --`

---

## Files Affected

- **Create:** `src/github/languages.rs` — fetch_languages()
- **Create:** `src/github/contributors.rs` — fetch_contributors()
- **Create:** `src/github/code_frequency.rs` — fetch_code_frequency()
- **Modify:** `src/github/mod.rs` — add new module declarations
- **Modify:** `src/app.rs` — new state fields, update Message enum, update handler
- **Modify:** `src/main.rs` — expand tokio::join! from 6 to 9 calls
- **Modify:** `src/ui/repo_detail.rs` — rewrite header+heatmap into 4 bordered panels
