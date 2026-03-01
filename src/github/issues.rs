use anyhow::Result;
use chrono::{DateTime, Utc};
use super::GitHubClient;

#[derive(Debug, Clone)]
pub struct IssueInfo {
    pub number: u64,
    pub title: String,
    pub state: String,
    pub user: String,
    pub labels: Vec<String>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    pub html_url: String,
    pub comments: u32,
}

impl GitHubClient {
    pub async fn fetch_repo_issues(&self, owner: &str, repo: &str) -> Result<Vec<IssueInfo>> {
        let mut issues = Vec::new();
        let mut page = 1u32;
        loop {
            let batch: Vec<serde_json::Value> = self.octocrab.get(
                format!("/repos/{}/{}/issues", owner, repo),
                Some(&[("per_page", "100"), ("page", &page.to_string()), ("state", "all")]),
            ).await?;

            if batch.is_empty() { break; }
            let done = batch.len() < 100;

            for item in &batch {
                // GitHub's issues endpoint also returns PRs — skip them
                if item.get("pull_request").is_some() { continue; }
                issues.push(parse_issue(item));
            }

            if done { break; }
            page += 1;
        }
        Ok(issues)
    }
}

fn parse_issue(json: &serde_json::Value) -> IssueInfo {
    IssueInfo {
        number: json["number"].as_u64().unwrap_or(0),
        title: json["title"].as_str().unwrap_or("").to_string(),
        state: json["state"].as_str().unwrap_or("").to_string(),
        user: json["user"]["login"].as_str().unwrap_or("").to_string(),
        labels: json["labels"]
            .as_array()
            .map(|arr| arr.iter().filter_map(|l| l["name"].as_str().map(String::from)).collect())
            .unwrap_or_default(),
        created_at: json["created_at"].as_str().and_then(|s| s.parse().ok()),
        updated_at: json["updated_at"].as_str().and_then(|s| s.parse().ok()),
        html_url: json["html_url"].as_str().unwrap_or("").to_string(),
        comments: json["comments"].as_u64().unwrap_or(0) as u32,
    }
}
