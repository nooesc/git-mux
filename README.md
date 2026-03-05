# git-mux

A terminal UI dashboard for GitHub. Browse your repos, view commit history, PRs, issues, CI status, and contribution stats — all without leaving the terminal.

Built with Rust and [ratatui](https://github.com/ratatui/ratatui).

![Home view](assets/home.png)

![Repo detail view](assets/repo-detail.png)

## Features

- Profile overview with avatar, bio, and contribution heatmap
- Repository cards with stars, forks, issues, and PR counts
- Stats panel with commit streak, top languages, and aggregate repo metrics
- Filter repos by visibility, org, or search
- Repo detail view with commits, PRs, issues, CI runs, languages, and health metrics
- Start Work: clone a repo, create a branch, and open it in a tmux session — or continue locally in an existing checkout
- Notification indicator
- Startup cache for fast reloads
- Config-driven org/repo exclusions

## Setup

Requires a GitHub personal access token. On first run, git-mux will prompt you to enter one, or you can set it via the `GITHUB_TOKEN` environment variable.

```
cargo install --path .
git-mux
```

## Config

Configuration lives at `~/.config/git-mux/config.toml`. A default config is created on first run.

```toml
[general]
refresh_interval_secs = 60
default_view = "repos"

[orgs]
include = []          # only show these orgs (empty = all)
exclude = ["some-org"]

[repos]
exclude = ["owner/repo-name"]

[workspaces]
dir = "~/dev-mux"                              # where cloned workspaces live
source_dirs = ["~/dev-personal", "~/dev-work"]  # existing checkout dirs for "Continue locally"
cleanup_after_days = 7                          # auto-remove stale workspaces (0 to disable)
```

## Keybindings

| Key | Action |
|-----|--------|
| `j/k` `↑/↓` | Navigate up/down |
| `h/l` `←/→` | Navigate left/right |
| `Enter` | Open repo detail |
| `Tab` | Cycle filter / section |
| `v` | Toggle list/card view |
| `/` | Search |
| `n` | Notifications |
| `o` | Open in browser |
| `r` | Re-run CI |
| `q` | Back / quit |
| `w` | Start work (clone/branch or continue locally) |
| `?` | Help |
