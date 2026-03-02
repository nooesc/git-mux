use super::GitHubClient;
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrInfo {
    pub number: u64,
    pub title: String,
    pub repo_full_name: String,
    pub state: String,
    pub html_url: String,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    pub draft: bool,
    pub user: String,
    pub head_ref: String,
    pub base_ref: String,
    pub merged: bool,
    pub additions: u32,
    pub deletions: u32,
}

#[derive(Debug, Clone, Default)]
pub struct PrState {
    pub authored: Vec<PrInfo>,
    pub review_requested: Vec<PrInfo>,
}

impl GitHubClient {
    pub async fn fetch_prs(&self) -> Result<PrState> {
        let authored = self
            .search_prs(&format!("author:{} type:pr state:open", self.username))
            .await?;

        let review_requested = self
            .search_prs(&format!(
                "review-requested:{} type:pr state:open",
                self.username
            ))
            .await?;

        Ok(PrState {
            authored,
            review_requested,
        })
    }

    async fn search_prs(&self, query: &str) -> Result<Vec<PrInfo>> {
        let results: serde_json::Value = self
            .octocrab
            .get(
                "/search/issues",
                Some(&[("q", query), ("per_page", "100"), ("sort", "updated")]),
            )
            .await?;

        let items = results["items"].as_array().cloned().unwrap_or_default();

        Ok(items
            .iter()
            .map(|item| {
                let repo_url = item["repository_url"].as_str().unwrap_or("");
                let repo_name = repo_url
                    .strip_prefix("https://api.github.com/repos/")
                    .unwrap_or("")
                    .to_string();

                PrInfo {
                    number: item["number"].as_u64().unwrap_or(0),
                    title: item["title"].as_str().unwrap_or("").to_string(),
                    repo_full_name: repo_name,
                    state: item["state"].as_str().unwrap_or("open").to_string(),
                    html_url: item["html_url"].as_str().unwrap_or("").to_string(),
                    created_at: item["created_at"].as_str().and_then(|s| s.parse().ok()),
                    updated_at: item["updated_at"].as_str().and_then(|s| s.parse().ok()),
                    draft: item["draft"].as_bool().unwrap_or(false),
                    user: item["user"]["login"].as_str().unwrap_or("").to_string(),
                    head_ref: item["head"]["ref"].as_str().unwrap_or("").to_string(),
                    base_ref: item["base"]["ref"].as_str().unwrap_or("").to_string(),
                    merged: false,
                    additions: 0,
                    deletions: 0,
                }
            })
            .collect())
    }

    #[allow(dead_code)]
    pub async fn merge_pr(&self, owner: &str, repo: &str, number: u64) -> Result<()> {
        self.octocrab
            .pulls(owner, repo)
            .merge(number)
            .send()
            .await?;
        Ok(())
    }

    #[allow(dead_code)]
    pub async fn approve_pr(&self, owner: &str, repo: &str, number: u64) -> Result<()> {
        let body = serde_json::json!({ "event": "APPROVE" });
        let _: serde_json::Value = self
            .octocrab
            .post(
                format!("/repos/{}/{}/pulls/{}/reviews", owner, repo, number),
                Some(&body),
            )
            .await?;
        Ok(())
    }

    #[allow(dead_code)]
    pub async fn comment_on_pr(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        body: &str,
    ) -> Result<()> {
        let payload = serde_json::json!({ "body": body });
        let _: serde_json::Value = self
            .octocrab
            .post(
                format!("/repos/{}/{}/issues/{}/comments", owner, repo, number),
                Some(&payload),
            )
            .await?;
        Ok(())
    }

    /// Fetch all PRs for a specific repo (open + closed, most recent first).
    pub async fn fetch_repo_prs(&self, owner: &str, repo: &str) -> Result<Vec<PrInfo>> {
        let mut prs = Vec::new();
        let mut page = 1u32;
        loop {
            let batch: Vec<serde_json::Value> = self
                .octocrab
                .get(
                    format!("/repos/{}/{}/pulls", owner, repo),
                    Some(&[
                        ("per_page", "100"),
                        ("page", &page.to_string()),
                        ("state", "all"),
                        ("sort", "updated"),
                        ("direction", "desc"),
                    ]),
                )
                .await?;

            if batch.is_empty() {
                break;
            }
            let done = batch.len() < 100;

            for item in &batch {
                prs.push(PrInfo {
                    number: item["number"].as_u64().unwrap_or(0),
                    title: item["title"].as_str().unwrap_or("").to_string(),
                    repo_full_name: format!("{}/{}", owner, repo),
                    state: item["state"].as_str().unwrap_or("open").to_string(),
                    html_url: item["html_url"].as_str().unwrap_or("").to_string(),
                    created_at: item["created_at"].as_str().and_then(|s| s.parse().ok()),
                    updated_at: item["updated_at"].as_str().and_then(|s| s.parse().ok()),
                    draft: item["draft"].as_bool().unwrap_or(false),
                    user: item["user"]["login"].as_str().unwrap_or("").to_string(),
                    head_ref: item["head"]["ref"].as_str().unwrap_or("").to_string(),
                    base_ref: item["base"]["ref"].as_str().unwrap_or("").to_string(),
                    merged: item["merged_at"].is_string(),
                    additions: item["additions"].as_u64().unwrap_or(0) as u32,
                    deletions: item["deletions"].as_u64().unwrap_or(0) as u32,
                });
            }

            if done || page >= 5 {
                break;
            }
            page += 1;
        }
        Ok(prs)
    }
}
