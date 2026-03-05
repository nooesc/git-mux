# Workspace Command Center

## Problem

git-mux is read-only — you can browse repos, PRs, and issues but can't act on them. Starting work on an issue means manually cloning, creating a branch, setting up `.env` files, and opening a terminal. This friction breaks the flow.

## Design

### Config

```toml
[workspaces]
dir = "~/git-mux"                              # where workspaces are created
source_dirs = ["~/dev-personal", "~/dev-work"]  # existing repos with .env files
cleanup_after_days = 7                          # auto-remove idle workspaces, 0 = disabled
```

### Directory Layout

```
~/git-mux/
  └── owner/
      └── repo/
          ├── issue-123-fix-auth-timeout/   ← from issue
          ├── pr-456-some-title/            ← from PR
          └── my-feature-branch/            ← from Home screen
```

Each workspace is a full clone with its own branch checked out. Directory names are slugified separately from branch names — slashes in branch names are replaced with `-` in the directory name.

### Action Menu

**Repo detail view (Issues/PRs tabs):** Enter opens an inline popup instead of directly opening the browser:

```
┌────────────────────────┐
│  ▸ Open in browser     │
│    Start work          │
└────────────────────────┘
```

- j/k to navigate, Enter to select, Esc to dismiss
- "Open in browser" is the default (first) option, so Enter → Enter preserves the quick path
- "Start work" only appears on Issues and PRs tabs (CI/Commits tabs keep direct browser-open on Enter)

**Home screen:** Enter keeps its current behavior (drill into repo detail). A separate `w` key triggers "Start work" directly, prompting for a branch name.

### Start Work Flow

**From an Issue:**
1. If workspace already exists at `~/git-mux/owner/repo/issue-123-fix-auth-timeout/`, run `git fetch` to update, then jump to step 4
2. Clone repo into `~/git-mux/owner/repo/issue-123-fix-auth-timeout/`
3. Copy env files from source directory, `git checkout -b issue-123-fix-auth-timeout`
4. Open tmux window for the workspace directory

**From a PR:**
1. If workspace already exists, run `git fetch` to update the PR ref, then jump to step 4
2. Clone into `~/git-mux/owner/repo/pr-123-some-title/`
3. Copy env files from source directory, `git fetch origin pull/123/head:pr-123-some-title && git checkout pr-123-some-title`
4. Open tmux window

**From Home screen (`w` key):**
1. Show inline text input: `Branch name: ___________`
2. Validate branch name with `git check-ref-format` rules (no `..`, no trailing `.lock`, no control chars, max 200 chars)
3. If workspace already exists, jump to step 5
4. Clone into `~/git-mux/owner/repo/{dir-slug}/`, copy env files, `git checkout -b {branch-name}`
5. Open tmux window

### Branch Naming

- Issues: `issue-{number}-{slugified-title}` (auto-generated, e.g., `issue-123-fix-auth-timeout`)
- PRs: `pr-{number}-{slugified-title}` (checks out the PR's head ref)
- Home: user-provided via inline text input, validated against git ref format rules

Slugification (for directory names): lowercase, replace `/` and non-alphanumeric chars with `-`, collapse consecutive `-`, trim to 60 chars. The git branch name is kept as-is; only the directory name is slugified.

### Env File Copying

When a matching repo is found in `source_dirs`, copy files matching these patterns from the source root into the new workspace:

- `.env*` (`.env`, `.env.local`, `.env.development`, etc.)

**Not copied:** `node_modules/`, `build/`, `dist/`, `.git/`, any file over 1MB, symlinks.

This is deliberately conservative. The goal is to copy secrets/config, not rebuild artifacts.

### Source Directory Matching

To find the right source repo for env file copying:

1. Walk each directory in `source_dirs` up to two levels deep (e.g., `~/dev-work/repo/` or `~/dev-work/org/repo/`)
2. For each candidate, run `git -C <dir> remote get-url origin` and compare against `owner/repo`
3. First match wins

No directory-name fallback — matching is always by git remote to prevent wrong-repo collisions (e.g., multiple repos named `api`).

### Clone Protocol

Detect the user's preferred git protocol from the source directory's remote URL:
- If source remote is `git@github.com:owner/repo.git` → clone via SSH
- If source remote is `https://github.com/owner/repo` → clone via HTTPS
- If no source directory found → default to HTTPS (`https://github.com/owner/repo.git`)

### Clone Progress

During clone, the TUI shows a status message: `Cloning owner/repo...` with a spinner. On failure, display the error in the TUI status line with which step failed (clone/copy/checkout/tmux).

### Workspace Cleanup

On app startup, scan `workspaces.dir` for workspace directories. For each workspace:

1. Find the most recently modified file (recursive) to determine true last-activity time
2. Check `git status --porcelain` — if the workspace has uncommitted changes, skip it regardless of age
3. If older than `cleanup_after_days` AND no tmux pane has its cwd set to this workspace → remove it
4. If a tmux pane is using this directory → skip, regardless of file age

Cleanup runs once per launch, not continuously. `cleanup_after_days = 0` disables cleanup entirely.

### Tmux Integration

- Open workspace: find an existing tmux pane whose cwd matches the workspace directory. If found, select that window. If not, create a new window with `tmux new-window -n <name> -c <workspace-dir>`
- Detection: use `tmux list-panes -a -F '#{pane_current_path}'` to match by working directory, not window name (avoids name collisions across repos)
- If git-mux detects it is not running inside tmux (`$TMUX` env var absent), skip tmux window creation and show the workspace path in the status line instead

### TUI State Changes

New state additions:
- `ActionMenu` overlay state (selected action index, target item)
- `BranchNameInput` state (for Home screen `w` key flow)
- `WorkspaceOp` state (current step: cloning/copying/checking-out, status message, error)

New `Message` variants:
- `ShowActionMenu` — triggered by Enter on an Issue/PR item
- `ActionMenuSelect` — triggered by Enter within the action menu
- `ActionMenuDismiss` — triggered by Esc within the action menu
- `StartWork` — begins the clone + branch + tmux flow
- `WorkspaceReady` / `WorkspaceError` — async callbacks when the operation completes or fails
- `BranchNameSubmit` — user confirms branch name on Home screen

### Implementation Approach

Shell out to `git` and `tmux` via `std::process::Command`. No new dependencies (no libgit2, no tmux crate). git-mux already shells out to `gh auth token` — same pattern.

Key commands:
- `git clone <url> <workspace-dir>`
- `git checkout -b <branch-name>`
- `git fetch origin pull/<number>/head:<branch-name>`
- `git -C <dir> remote get-url origin` (for source matching + protocol detection)
- `git status --porcelain` (for cleanup dirty-check)
- `tmux list-panes -a -F '#{pane_current_path}'` (for workspace detection)
- `tmux new-window -n <name> -c <workspace-dir>`

### Testing

Key areas requiring tests:

- **Slug generation:** branch names with slashes, unicode, long titles, special chars
- **Source matching:** SSH vs HTTPS remote URLs, repos at different depths in source_dirs, no-match fallback
- **Env file copying:** `.env*` glob correctness, size cap enforcement, symlink skip
- **Cleanup safety:** skip dirty workspaces, skip workspaces with active tmux panes, respect `cleanup_after_days = 0`
- **Idempotency:** existing workspace reopens instead of failing, `git fetch` updates on reopen
- **Branch validation:** reject invalid ref names, handle max length, sanitize directory slug separately
