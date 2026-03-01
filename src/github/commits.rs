use anyhow::Result;
use chrono::{DateTime, Utc};
use super::GitHubClient;

#[derive(Debug, Clone)]
pub struct WeeklyCommitActivity {
    pub week_start: DateTime<Utc>,
    pub total: u32,
    pub days: [u32; 7], // Sun-Sat
}

#[derive(Debug, Clone)]
pub struct CommitInfo {
    pub sha: String,
    pub short_sha: String,
    pub message: String,
    pub author: String,
    pub date: DateTime<Utc>,
    pub parents: Vec<String>,
    pub html_url: String,
}

impl GitHubClient {
    pub async fn fetch_commit_activity(&self, owner: &str, repo: &str) -> Result<Vec<WeeklyCommitActivity>> {
        let result: serde_json::Value = self.octocrab.get(
            format!("/repos/{}/{}/stats/commit_activity", owner, repo),
            None::<&()>,
        ).await?;

        let mut weeks = Vec::new();
        if let Some(arr) = result.as_array() {
            for item in arr {
                let timestamp = item["week"].as_i64().unwrap_or(0);
                let week_start = DateTime::from_timestamp(timestamp, 0).unwrap_or_default();
                let total = item["total"].as_u64().unwrap_or(0) as u32;
                let mut days = [0u32; 7];
                if let Some(day_arr) = item["days"].as_array() {
                    for (i, d) in day_arr.iter().enumerate().take(7) {
                        days[i] = d.as_u64().unwrap_or(0) as u32;
                    }
                }
                weeks.push(WeeklyCommitActivity { week_start, total, days });
            }
        }
        Ok(weeks)
    }

    pub async fn fetch_repo_commits(&self, owner: &str, repo: &str) -> Result<Vec<CommitInfo>> {
        let result: Vec<serde_json::Value> = self.octocrab.get(
            format!("/repos/{}/{}/commits", owner, repo),
            Some(&[("per_page", "50")]),
        ).await?;

        let mut commits = Vec::new();
        for item in &result {
            let sha = item["sha"].as_str().unwrap_or("").to_string();
            let short_sha = sha.chars().take(7).collect();
            let commit = &item["commit"];
            let message = commit["message"].as_str().unwrap_or("")
                .lines().next().unwrap_or("").to_string();
            let author = commit["author"]["name"].as_str().unwrap_or("").to_string();
            let date = commit["author"]["date"].as_str()
                .and_then(|s| s.parse::<DateTime<Utc>>().ok())
                .unwrap_or_default();
            let parents = item["parents"].as_array()
                .map(|arr| arr.iter().filter_map(|p| p["sha"].as_str().map(String::from)).collect())
                .unwrap_or_default();
            let html_url = item["html_url"].as_str().unwrap_or("").to_string();

            commits.push(CommitInfo { sha, short_sha, message, author, date, parents, html_url });
        }
        Ok(commits)
    }
}
