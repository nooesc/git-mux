use anyhow::{bail, Result};

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
}
