use anyhow::Result;
use chrono::{DateTime, Utc};
use super::GitHubClient;

#[derive(Debug, Clone)]
pub struct RepoInfo {
    pub full_name: String,
    pub description: Option<String>,
    pub language: Option<String>,
    pub stargazers_count: u32,
    pub forks_count: u32,
    pub open_issues_count: u32,
    pub pushed_at: Option<DateTime<Utc>>,
    pub html_url: String,
    pub is_fork: bool,
    pub is_private: bool,
    pub owner: String,
}

impl GitHubClient {
    /// Fetch all repos for the authenticated user (personal + all orgs).
    pub async fn fetch_all_repos(&self) -> Result<Vec<RepoInfo>> {
        let mut all_repos = Vec::new();

        // Fetch personal repos (paginated)
        let mut page = 1u32;
        loop {
            let repos: Vec<serde_json::Value> = self.octocrab.get(
                "/user/repos",
                Some(&[("per_page", "100"), ("page", &page.to_string()), ("sort", "pushed")]),
            ).await?;

            if repos.is_empty() {
                break;
            }

            for repo in &repos {
                all_repos.push(parse_repo(repo));
            }

            if repos.len() < 100 {
                break;
            }
            page += 1;
        }

        // Fetch org repos
        let orgs: Vec<serde_json::Value> = self.octocrab.get(
            "/user/orgs",
            None::<&()>,
        ).await?;

        for org in &orgs {
            if let Some(org_login) = org["login"].as_str() {
                let mut page = 1u32;
                loop {
                    let repos: Vec<serde_json::Value> = self.octocrab.get(
                        format!("/orgs/{}/repos", org_login),
                        Some(&[("per_page", "100"), ("page", &page.to_string()), ("sort", "pushed")]),
                    ).await?;

                    if repos.is_empty() {
                        break;
                    }

                    for repo in &repos {
                        all_repos.push(parse_repo(repo));
                    }

                    if repos.len() < 100 {
                        break;
                    }
                    page += 1;
                }
            }
        }

        // Deduplicate by full_name (personal repos can overlap with org repos)
        all_repos.sort_by(|a, b| a.full_name.cmp(&b.full_name));
        all_repos.dedup_by(|a, b| a.full_name == b.full_name);

        // Sort by most recently pushed
        all_repos.sort_by(|a, b| b.pushed_at.cmp(&a.pushed_at));

        Ok(all_repos)
    }
}

fn parse_repo(json: &serde_json::Value) -> RepoInfo {
    RepoInfo {
        full_name: json["full_name"].as_str().unwrap_or("").to_string(),
        description: json["description"].as_str().map(|s| s.to_string()),
        language: json["language"].as_str().map(|s| s.to_string()),
        stargazers_count: json["stargazers_count"].as_u64().unwrap_or(0) as u32,
        forks_count: json["forks_count"].as_u64().unwrap_or(0) as u32,
        open_issues_count: json["open_issues_count"].as_u64().unwrap_or(0) as u32,
        pushed_at: json["pushed_at"]
            .as_str()
            .and_then(|s| s.parse::<DateTime<Utc>>().ok()),
        html_url: json["html_url"].as_str().unwrap_or("").to_string(),
        is_fork: json["fork"].as_bool().unwrap_or(false),
        is_private: json["private"].as_bool().unwrap_or(false),
        owner: json["owner"]["login"].as_str().unwrap_or("").to_string(),
    }
}
