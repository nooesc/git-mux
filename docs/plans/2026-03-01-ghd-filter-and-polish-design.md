# GHD Filter Bar, Selection Highlight & Config Exclusions Design

## Goal

Add three features to the ghd TUI: (1) background fill on selected repo cards/rows, (2) an inline filter bar for toggling between All/Public/Private/per-org views, and (3) config-driven exclusion of orgs and repos.

## Architecture

All three features are additive changes to existing modules — no new files needed. The filter bar introduces a `HomeFocus` concept (filter bar vs repo grid) to the home screen navigation model. Config exclusions are applied at data load time so excluded repos never enter app state.

## Feature 1: Selected Card Background Fill

**What:** When a repo card or list row is selected, render a subtle background tint behind the entire widget — not just a border color change.

**Card view (`render_card`):**
- Selected: border cyan + `bg(Color::Rgb(30, 40, 50))` on the Block
- Unselected: border dark gray, no background

**List view (`render_list_view`):**
- Selected row: full-width background tint `bg(Color::Rgb(30, 40, 50))` + cyan text
- Unselected row: no background

**Files:** `src/ui/home.rs` (render_card, render_list_view)

## Feature 2: Inline Filter Bar

**Layout change:**

```
[avatar 24col] [info 20col] [graph fill]
[  All  | Public | Private | OrgA | OrgB  ]  <- 1-row filter bar
[repo cards / list ...]
```

The filter bar sits between the profile/graph section and the repo grid. It's a single row showing filter options derived from the loaded data.

### Navigation Model

New enum `HomeFocus { FilterBar, Repos }` tracks which section has keyboard focus on the home screen.

- **j/k (up/down):** When focus is `Repos` and cursor is at the top, `k` moves focus to `FilterBar`. When focus is `FilterBar`, `j` moves focus to `Repos`.
- **h/l (left/right):** When focus is `FilterBar`, h/l cycles through filter options and immediately applies the filter. When focus is `Repos`, h/l navigates card columns (unchanged).
- **Enter on FilterBar:** Drops focus to `Repos` (convenience).

### Filter Options

Dynamic list built from loaded repos:
1. `All` — always present
2. `Public` — always present
3. `Private` — always present
4. One entry per unique org (owner that isn't the authenticated user)

### State Changes (`AppState`)

```rust
pub home_focus: HomeFocus,    // FilterBar or Repos
pub repo_filter: RepoFilter,  // All, Public, Private, Org(String)
pub filter_index: usize,      // cursor in the filter bar
```

### Filtering Logic

`filtered_repos()` applies both search query AND repo_filter:
- `All` — no visibility filter
- `Public` — `!repo.is_private`
- `Private` — `repo.is_private`
- `Org(name)` — `repo.owner == name`

### Rendering

Filter bar rendered as a row of spans:
- Active filter: cyan + bold
- Focused (cursor on it): underlined
- Inactive: dark gray
- Separator: ` | ` in dark gray

## Feature 3: Config Exclusions

**Config format** (`~/.config/ghd/config.toml`):

```toml
[orgs]
exclude = ["boring-corp"]

[repos]
exclude = ["owner/repo-name", "org/internal-tool"]
```

### Config Struct Changes

Add to `config.rs`:

```rust
#[derive(Debug, Deserialize, Serialize, Default)]
pub struct RepoConfig {
    #[serde(default)]
    pub exclude: Vec<String>,
}
```

Add `pub repos: RepoConfig` field to `Config`.

### Exclusion Application

Store config in `AppState`. When `ReposLoaded` is processed in `update()`, filter out repos where:
- `repo.owner` matches any entry in `config.orgs.exclude`
- `repo.full_name` matches any entry in `config.repos.exclude`

This keeps excluded repos out of the app entirely — they don't appear in any filter view.

## Key Bindings (unchanged)

No new keys. `h/l/j/k` gain context-awareness on the home screen based on `HomeFocus`. The `v` key still toggles card/list view. Search `/` still filters within the current visibility filter.

## Files Modified

| File | Changes |
|------|---------|
| `src/app.rs` | Add `HomeFocus`, `RepoFilter` enums, new state fields, update `filtered_repos()`, store config, handle exclusions in `ReposLoaded` |
| `src/ui/home.rs` | Background fill on selected cards/rows, render filter bar, adjust layout for filter row |
| `src/config.rs` | Add `RepoConfig` struct, add `repos` field to `Config` |
| `src/main.rs` | Pass config into `AppState`, adjust navigation key handling for `HomeFocus` |
