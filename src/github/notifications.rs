use super::GitHubClient;
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    pub id: String,
    pub reason: String,
    pub subject_title: String,
    pub subject_type: String, // PullRequest, Issue, Release, CheckSuite, etc.
    pub repo_full_name: String,
    pub updated_at: Option<DateTime<Utc>>,
    pub unread: bool,
    pub url: Option<String>,
}

impl GitHubClient {
    pub async fn fetch_notifications(&self) -> Result<Vec<Notification>> {
        let items: Vec<serde_json::Value> = self
            .octocrab
            .get("/notifications", Some(&[("per_page", "50")]))
            .await?;

        Ok(items
            .iter()
            .map(|item| Notification {
                id: item["id"].as_str().unwrap_or("").to_string(),
                reason: item["reason"].as_str().unwrap_or("").to_string(),
                subject_title: item["subject"]["title"].as_str().unwrap_or("").to_string(),
                subject_type: item["subject"]["type"].as_str().unwrap_or("").to_string(),
                repo_full_name: item["repository"]["full_name"]
                    .as_str()
                    .unwrap_or("")
                    .to_string(),
                updated_at: item["updated_at"].as_str().and_then(|s| s.parse().ok()),
                unread: item["unread"].as_bool().unwrap_or(false),
                url: item["subject"]["url"].as_str().map(|s| {
                    s.replace("api.github.com/repos", "github.com")
                        .replace("/pulls/", "/pull/")
                }),
            })
            .collect())
    }

    pub async fn mark_notification_read(&self, thread_id: &str) -> Result<()> {
        let uri = format!("https://api.github.com/notifications/threads/{}", thread_id);
        let _response = self.octocrab._patch(uri, None::<&()>).await?;
        Ok(())
    }
}
