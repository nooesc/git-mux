use anyhow::{bail, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Slugify a string for use as a directory name.
/// Lowercase, replace non-alphanumeric with `-`, collapse consecutive `-`, trim to 60 chars.
pub fn slugify(s: &str) -> String {
    let slug: String = s
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let mut result = String::new();
    let mut prev_dash = false;
    for c in slug.chars() {
        if c == '-' {
            if !prev_dash {
                result.push(c);
            }
            prev_dash = true;
        } else {
            result.push(c);
            prev_dash = false;
        }
    }
    let trimmed = result.trim_end_matches('-');
    if trimmed.len() > 60 {
        trimmed[..60].trim_end_matches('-').to_string()
    } else {
        trimmed.to_string()
    }
}

/// Generate branch name for an issue: issue-{number}-{slugified-title}
pub fn issue_branch_name(number: u64, title: &str) -> String {
    format!("issue-{}-{}", number, slugify(title))
}

/// Generate directory slug for a PR: pr-{number}-{slugified-title}
pub fn pr_dir_slug(number: u64, title: &str) -> String {
    format!("pr-{}-{}", number, slugify(title))
}

/// Validate a user-provided branch name against git ref format rules.
pub fn validate_branch_name(name: &str) -> Result<()> {
    if name.is_empty() {
        bail!("Branch name cannot be empty");
    }
    if name.len() > 200 {
        bail!("Branch name too long (max 200 chars)");
    }
    if name.contains("..") {
        bail!("Branch name cannot contain '..'");
    }
    if name.ends_with(".lock") {
        bail!("Branch name cannot end with '.lock'");
    }
    if name.contains(' ') || name.contains('~') || name.contains('^') || name.contains(':') {
        bail!("Branch name contains invalid characters");
    }
    if name.starts_with('-') || name.starts_with('.') {
        bail!("Branch name cannot start with '-' or '.'");
    }
    if name.ends_with('.') || name.ends_with('/') {
        bail!("Branch name cannot end with '.' or '/'");
    }
    if name.contains("//") {
        bail!("Branch name cannot contain consecutive slashes");
    }
    Ok(())
}

/// Extract `owner/repo` from a git remote URL (SSH or HTTPS).
fn parse_remote_owner_repo(url: &str) -> Option<(String, String)> {
    // SSH: git@github.com:owner/repo.git
    if let Some(rest) = url.strip_prefix("git@github.com:") {
        let clean = rest.trim_end_matches(".git");
        let parts: Vec<&str> = clean.splitn(2, '/').collect();
        if parts.len() == 2 {
            return Some((parts[0].to_string(), parts[1].to_string()));
        }
    }
    // HTTPS: https://github.com/owner/repo.git
    if url.contains("github.com/") {
        let after = url.split("github.com/").nth(1)?;
        let clean = after.trim_end_matches(".git");
        let parts: Vec<&str> = clean.splitn(2, '/').collect();
        if parts.len() == 2 {
            return Some((parts[0].to_string(), parts[1].to_string()));
        }
    }
    None
}

/// Get the origin remote URL for a directory, if it's a git repo.
pub fn get_remote_url(dir: &Path) -> Option<String> {
    let output = Command::new("git")
        .args(["-C", &dir.to_string_lossy(), "remote", "get-url", "origin"])
        .output()
        .ok()?;
    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        None
    }
}

fn check_remote_match(dir: &Path, owner: &str, repo: &str) -> bool {
    if let Some(url) = get_remote_url(dir) {
        if let Some((remote_owner, remote_repo)) = parse_remote_owner_repo(&url) {
            return remote_owner.eq_ignore_ascii_case(owner)
                && remote_repo.eq_ignore_ascii_case(repo);
        }
    }
    false
}

/// Find a source repository in `source_dirs` that matches `owner/repo`.
/// Scans up to two levels deep. Matches by git remote URL.
pub fn find_source_repo(source_dirs: &[String], owner: &str, repo: &str) -> Option<PathBuf> {
    for source_dir in source_dirs {
        let expanded = shellexpand::tilde(source_dir);
        let base = PathBuf::from(expanded.as_ref());
        if !base.is_dir() {
            continue;
        }
        // Level 1: direct children
        if let Ok(entries) = std::fs::read_dir(&base) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                if check_remote_match(&path, owner, repo) {
                    return Some(path);
                }
                // Level 2: children of children
                if let Ok(sub_entries) = std::fs::read_dir(&path) {
                    for sub_entry in sub_entries.flatten() {
                        let sub_path = sub_entry.path();
                        if sub_path.is_dir() && check_remote_match(&sub_path, owner, repo) {
                            return Some(sub_path);
                        }
                    }
                }
            }
        }
    }
    None
}

/// Copy `.env*` files from source to destination directory.
/// Skips files over 1MB and symlinks.
pub fn copy_env_files(src: &Path, dst: &Path) -> Result<u32> {
    let mut count = 0;
    if let Ok(entries) = std::fs::read_dir(src) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let name_str = name.to_string_lossy();

            if !name_str.starts_with(".env") {
                continue;
            }
            if path
                .symlink_metadata()
                .map(|m| m.file_type().is_symlink())
                .unwrap_or(false)
            {
                continue;
            }
            if path.metadata().map(|m| m.len()).unwrap_or(0) > 1_048_576 {
                continue;
            }
            std::fs::copy(&path, dst.join(&name))?;
            count += 1;
        }
    }
    Ok(count)
}

/// Detect clone protocol from source repo's remote URL.
pub fn clone_url(source_remote: Option<&str>, owner: &str, repo: &str) -> String {
    match source_remote {
        Some(url) if url.starts_with("git@") => {
            format!("git@github.com:{}/{}.git", owner, repo)
        }
        _ => {
            format!("https://github.com/{}/{}.git", owner, repo)
        }
    }
}

/// Central struct for workspace operations.
pub struct WorkspaceOps {
    pub base_dir: String,
}

impl WorkspaceOps {
    pub fn new(base_dir: String) -> Self {
        Self { base_dir }
    }

    /// Get the workspace directory path for a given owner/repo/slug.
    pub fn workspace_dir(&self, owner: &str, repo: &str, dir_slug: &str) -> PathBuf {
        let expanded = shellexpand::tilde(&self.base_dir);
        PathBuf::from(expanded.as_ref())
            .join(owner)
            .join(repo)
            .join(dir_slug)
    }

    /// Check if a workspace already exists.
    pub fn workspace_exists(&self, owner: &str, repo: &str, dir_slug: &str) -> bool {
        self.workspace_dir(owner, repo, dir_slug).is_dir()
    }

    /// Clone a repo into the workspace directory.
    pub fn clone_repo(
        &self,
        url: &str,
        owner: &str,
        repo: &str,
        dir_slug: &str,
    ) -> Result<PathBuf> {
        let ws_dir = self.workspace_dir(owner, repo, dir_slug);
        if let Some(parent) = ws_dir.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let output = Command::new("git")
            .args(["clone", url, &ws_dir.to_string_lossy()])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("git clone failed: {}", stderr.trim());
        }
        Ok(ws_dir)
    }

    /// Create and checkout a new branch in the workspace.
    pub fn checkout_new_branch(ws_dir: &Path, branch: &str) -> Result<()> {
        let output = Command::new("git")
            .args(["checkout", "-b", branch])
            .current_dir(ws_dir)
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("git checkout -b failed: {}", stderr.trim());
        }
        Ok(())
    }

    /// Fetch and checkout a PR by number.
    pub fn checkout_pr(ws_dir: &Path, pr_number: u64, local_branch: &str) -> Result<()> {
        let output = Command::new("git")
            .args([
                "fetch",
                "origin",
                &format!("pull/{}/head:{}", pr_number, local_branch),
            ])
            .current_dir(ws_dir)
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("git fetch PR failed: {}", stderr.trim());
        }

        let output = Command::new("git")
            .args(["checkout", local_branch])
            .current_dir(ws_dir)
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("git checkout failed: {}", stderr.trim());
        }
        Ok(())
    }

    /// Fetch latest in an existing workspace.
    pub fn fetch_latest(ws_dir: &Path) -> Result<()> {
        let output = Command::new("git")
            .args(["fetch", "--all"])
            .current_dir(ws_dir)
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("git fetch failed: {}", stderr.trim());
        }
        Ok(())
    }

    /// Clean up workspaces older than `days` that are clean and have no active tmux panes.
    /// Returns the number of workspaces removed.
    pub fn cleanup_old_workspaces(&self, days: u64) -> Result<u32> {
        if days == 0 {
            return Ok(0);
        }

        let expanded = shellexpand::tilde(&self.base_dir);
        let base = PathBuf::from(expanded.as_ref());
        if !base.is_dir() {
            return Ok(0);
        }

        let cutoff = std::time::SystemTime::now()
            - std::time::Duration::from_secs(days * 24 * 60 * 60);

        // Get all tmux pane cwds for safety check
        let active_paths = get_tmux_pane_paths();

        let mut removed = 0;

        // Walk owner/repo/workspace structure
        for owner_entry in std::fs::read_dir(&base)?.flatten() {
            if !owner_entry.path().is_dir() {
                continue;
            }
            for repo_entry in std::fs::read_dir(owner_entry.path())?.flatten() {
                if !repo_entry.path().is_dir() {
                    continue;
                }
                for ws_entry in std::fs::read_dir(repo_entry.path())?.flatten() {
                    let ws_path = ws_entry.path();
                    if !ws_path.is_dir() {
                        continue;
                    }

                    // Skip if tmux pane is using this directory
                    let ws_str = ws_path.to_string_lossy().to_string();
                    if active_paths.contains(&ws_str) {
                        continue;
                    }

                    // Skip if workspace has uncommitted changes
                    if is_workspace_dirty(&ws_path) {
                        continue;
                    }

                    // Check most recent file modification time (recursive)
                    if let Some(latest) = most_recent_mtime(&ws_path) {
                        if latest < cutoff {
                            std::fs::remove_dir_all(&ws_path)?;
                            removed += 1;
                        }
                    }
                }
            }
        }
        Ok(removed)
    }
}

/// Check if running inside tmux.
pub fn is_inside_tmux() -> bool {
    std::env::var("TMUX").is_ok()
}

/// Open a tmux window for a workspace directory.
/// If a pane with this cwd already exists, select that window instead.
pub fn open_tmux_window(ws_dir: &Path, name: &str) -> Result<()> {
    // Check if a pane already has this cwd
    let output = Command::new("tmux")
        .args([
            "list-panes",
            "-a",
            "-F",
            "#{session_name}:#{window_index} #{pane_current_path}",
        ])
        .output()?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let ws_str = ws_dir.to_string_lossy();
        for line in stdout.lines() {
            if let Some((target, path)) = line.split_once(' ') {
                if path == ws_str.as_ref() {
                    // Select existing window
                    let _ = Command::new("tmux")
                        .args(["select-window", "-t", target])
                        .output();
                    return Ok(());
                }
            }
        }
    }

    // Create new window
    let output = Command::new("tmux")
        .args([
            "new-window",
            "-n",
            name,
            "-c",
            &ws_dir.to_string_lossy(),
        ])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("tmux new-window failed: {}", stderr.trim());
    }
    Ok(())
}

/// Get all tmux pane current working directories.
fn get_tmux_pane_paths() -> Vec<String> {
    let output = Command::new("tmux")
        .args(["list-panes", "-a", "-F", "#{pane_current_path}"])
        .output();

    match output {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
            .lines()
            .map(String::from)
            .collect(),
        _ => Vec::new(),
    }
}

/// Check if a git workspace has uncommitted changes.
fn is_workspace_dirty(dir: &Path) -> bool {
    let output = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(dir)
        .output();

    match output {
        Ok(o) if o.status.success() => !String::from_utf8_lossy(&o.stdout).trim().is_empty(),
        _ => true, // If we can't determine, treat as dirty (safe default)
    }
}

/// Find the most recently modified file in a directory tree.
fn most_recent_mtime(dir: &Path) -> Option<std::time::SystemTime> {
    let mut latest = None;
    walk_dir_for_mtime(dir, &mut latest);
    latest
}

/// Simple recursive directory walker for finding most recent mtime (skips .git for performance).
fn walk_dir_for_mtime(dir: &Path, latest: &mut Option<std::time::SystemTime>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let name = entry.file_name();
                if name == ".git" {
                    continue;
                }
                walk_dir_for_mtime(&path, latest);
            } else if let Ok(meta) = path.metadata() {
                if let Ok(modified) = meta.modified() {
                    *latest = Some(match *latest {
                        Some(prev) if modified > prev => modified,
                        Some(prev) => prev,
                        None => modified,
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slugify_simple() {
        assert_eq!(slugify("Fix auth timeout"), "fix-auth-timeout");
    }

    #[test]
    fn test_slugify_special_chars() {
        assert_eq!(slugify("feat: add OAuth 2.0!"), "feat-add-oauth-2-0");
    }

    #[test]
    fn test_slugify_slashes() {
        assert_eq!(slugify("feature/my-branch"), "feature-my-branch");
    }

    #[test]
    fn test_slugify_consecutive_dashes() {
        assert_eq!(slugify("fix---multiple---dashes"), "fix-multiple-dashes");
    }

    #[test]
    fn test_slugify_trim_length() {
        let long_title = "a".repeat(100);
        let slug = slugify(&long_title);
        assert!(slug.len() <= 60);
    }

    #[test]
    fn test_slugify_trim_trailing_dash() {
        assert_eq!(slugify("hello-world-"), "hello-world");
    }

    #[test]
    fn test_issue_branch_name() {
        assert_eq!(
            issue_branch_name(123, "Fix auth timeout"),
            "issue-123-fix-auth-timeout"
        );
    }

    #[test]
    fn test_pr_dir_slug() {
        assert_eq!(
            pr_dir_slug(456, "feat: campaign post rescraping"),
            "pr-456-feat-campaign-post-rescraping"
        );
    }

    #[test]
    fn test_validate_branch_name_valid() {
        assert!(validate_branch_name("my-feature").is_ok());
        assert!(validate_branch_name("feat/my-branch").is_ok());
    }

    #[test]
    fn test_validate_branch_name_invalid() {
        assert!(validate_branch_name("").is_err());
        assert!(validate_branch_name("my..branch").is_err());
        assert!(validate_branch_name("branch.lock").is_err());
        assert!(validate_branch_name("branch with spaces").is_err());
        assert!(validate_branch_name(&"a".repeat(201)).is_err());
    }

    #[test]
    fn test_find_source_repo_by_remote() {
        let tmp = tempfile::tempdir().unwrap();
        let repo_dir = tmp.path().join("my-project");
        std::fs::create_dir_all(&repo_dir).unwrap();

        Command::new("git")
            .args(["init"])
            .current_dir(&repo_dir)
            .output()
            .unwrap();
        Command::new("git")
            .args([
                "remote",
                "add",
                "origin",
                "https://github.com/owner/my-project.git",
            ])
            .current_dir(&repo_dir)
            .output()
            .unwrap();

        let source_dirs = vec![tmp.path().to_string_lossy().to_string()];
        let result = find_source_repo(&source_dirs, "owner", "my-project");
        assert!(result.is_some());
        assert_eq!(result.unwrap(), repo_dir);
    }

    #[test]
    fn test_find_source_repo_ssh_remote() {
        let tmp = tempfile::tempdir().unwrap();
        let repo_dir = tmp.path().join("my-project");
        std::fs::create_dir_all(&repo_dir).unwrap();

        Command::new("git")
            .args(["init"])
            .current_dir(&repo_dir)
            .output()
            .unwrap();
        Command::new("git")
            .args([
                "remote",
                "add",
                "origin",
                "git@github.com:owner/my-project.git",
            ])
            .current_dir(&repo_dir)
            .output()
            .unwrap();

        let source_dirs = vec![tmp.path().to_string_lossy().to_string()];
        let result = find_source_repo(&source_dirs, "owner", "my-project");
        assert!(result.is_some());
    }

    #[test]
    fn test_find_source_repo_two_levels_deep() {
        let tmp = tempfile::tempdir().unwrap();
        let repo_dir = tmp.path().join("org").join("my-project");
        std::fs::create_dir_all(&repo_dir).unwrap();

        Command::new("git")
            .args(["init"])
            .current_dir(&repo_dir)
            .output()
            .unwrap();
        Command::new("git")
            .args([
                "remote",
                "add",
                "origin",
                "https://github.com/owner/my-project.git",
            ])
            .current_dir(&repo_dir)
            .output()
            .unwrap();

        let source_dirs = vec![tmp.path().to_string_lossy().to_string()];
        let result = find_source_repo(&source_dirs, "owner", "my-project");
        assert!(result.is_some());
    }

    #[test]
    fn test_find_source_repo_no_match() {
        let tmp = tempfile::tempdir().unwrap();
        let source_dirs = vec![tmp.path().to_string_lossy().to_string()];
        let result = find_source_repo(&source_dirs, "owner", "nonexistent");
        assert!(result.is_none());
    }

    #[test]
    fn test_copy_env_files() {
        let src = tempfile::tempdir().unwrap();
        let dst = tempfile::tempdir().unwrap();

        std::fs::write(src.path().join(".env"), "SECRET=123").unwrap();
        std::fs::write(src.path().join(".env.local"), "LOCAL=456").unwrap();
        std::fs::write(src.path().join(".env.development"), "DEV=789").unwrap();
        std::fs::write(src.path().join("README.md"), "hello").unwrap();

        copy_env_files(src.path(), dst.path()).unwrap();

        assert!(dst.path().join(".env").exists());
        assert!(dst.path().join(".env.local").exists());
        assert!(dst.path().join(".env.development").exists());
        assert!(!dst.path().join("README.md").exists());
    }

    #[test]
    fn test_copy_env_files_skips_large() {
        let src = tempfile::tempdir().unwrap();
        let dst = tempfile::tempdir().unwrap();

        let big_content = "X".repeat(1_100_000);
        std::fs::write(src.path().join(".env.huge"), &big_content).unwrap();
        std::fs::write(src.path().join(".env"), "SMALL=yes").unwrap();

        copy_env_files(src.path(), dst.path()).unwrap();

        assert!(dst.path().join(".env").exists());
        assert!(!dst.path().join(".env.huge").exists());
    }

    #[test]
    fn test_clone_url_from_ssh_source() {
        assert_eq!(
            clone_url(Some("git@github.com:owner/repo.git"), "owner", "repo"),
            "git@github.com:owner/repo.git"
        );
    }

    #[test]
    fn test_clone_url_from_https_source() {
        assert_eq!(
            clone_url(
                Some("https://github.com/owner/repo.git"),
                "owner",
                "repo"
            ),
            "https://github.com/owner/repo.git"
        );
    }

    #[test]
    fn test_clone_url_no_source() {
        assert_eq!(
            clone_url(None, "owner", "repo"),
            "https://github.com/owner/repo.git"
        );
    }

    #[test]
    fn test_workspace_path() {
        let ws = WorkspaceOps::new("/tmp/workspaces".to_string());
        let path = ws.workspace_dir("owner", "repo", "issue-123-fix-it");
        assert_eq!(
            path,
            PathBuf::from("/tmp/workspaces/owner/repo/issue-123-fix-it")
        );
    }

    #[test]
    fn test_workspace_exists_false() {
        let ws = WorkspaceOps::new("/tmp/nonexistent-ws-test".to_string());
        assert!(!ws.workspace_exists("owner", "repo", "nope"));
    }

    #[test]
    fn test_is_inside_tmux() {
        // Just verify it returns a bool without panicking
        let _ = is_inside_tmux();
    }

    #[test]
    fn test_cleanup_skips_when_disabled() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = WorkspaceOps::new(tmp.path().to_string_lossy().to_string());
        let removed = ws.cleanup_old_workspaces(0).unwrap();
        assert_eq!(removed, 0);
    }

    #[test]
    fn test_cleanup_skips_dirty_workspace() {
        let tmp = tempfile::tempdir().unwrap();
        let ws_dir = tmp.path().join("owner").join("repo").join("feature");
        std::fs::create_dir_all(&ws_dir).unwrap();

        // Init git repo with uncommitted changes
        Command::new("git")
            .args(["init"])
            .current_dir(&ws_dir)
            .output()
            .unwrap();
        Command::new("git")
            .args([
                "-c",
                "user.name=test",
                "-c",
                "user.email=test@test.com",
                "commit",
                "--allow-empty",
                "-m",
                "init",
            ])
            .current_dir(&ws_dir)
            .output()
            .unwrap();
        std::fs::write(ws_dir.join("dirty.txt"), "uncommitted").unwrap();

        let ws = WorkspaceOps::new(tmp.path().to_string_lossy().to_string());
        let removed = ws.cleanup_old_workspaces(1).unwrap();
        assert_eq!(removed, 0);
        assert!(ws_dir.exists());
    }
}
