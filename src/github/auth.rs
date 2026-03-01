use anyhow::{bail, Context, Result};
use std::process::Command;

/// Extract GitHub token. Priority:
/// 1. `gh auth token` subprocess
/// 2. GITHUB_TOKEN env var
pub fn get_token() -> Result<String> {
    if let Ok(token) = get_token_from_gh_cli() {
        return Ok(token);
    }
    if let Ok(token) = std::env::var("GITHUB_TOKEN") {
        if !token.is_empty() {
            return Ok(token);
        }
    }
    bail!(
        "No GitHub token found. Either:\n\
         1. Install and authenticate gh CLI: `gh auth login`\n\
         2. Set GITHUB_TOKEN environment variable"
    )
}

fn get_token_from_gh_cli() -> Result<String> {
    let output = Command::new("gh")
        .args(["auth", "token"])
        .output()
        .context("Failed to run `gh auth token`. Is gh CLI installed?")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("gh auth token failed: {}", stderr.trim());
    }

    let token = String::from_utf8(output.stdout)
        .context("gh auth token output was not valid UTF-8")?
        .trim()
        .to_string();

    if token.is_empty() {
        bail!("gh auth token returned empty string");
    }

    Ok(token)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_token_returns_non_empty_string() {
        // This test requires gh CLI to be authenticated on the machine.
        let token = get_token().expect("Should get a token from gh CLI or env");
        assert!(!token.is_empty());
        assert!(
            token.starts_with("ghp_")
                || token.starts_with("gho_")
                || token.starts_with("github_pat_"),
            "Token should start with a known GitHub prefix, got: {}...",
            &token[..8]
        );
    }
}
