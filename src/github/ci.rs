use super::GitHubClient;
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowRun {
    pub id: u64,
    pub name: String,
    pub repo_full_name: String,
    pub head_branch: String,
    pub status: String,             // queued, in_progress, completed
    pub conclusion: Option<String>, // success, failure, cancelled, etc.
    pub created_at: Option<DateTime<Utc>>,
    pub html_url: String,
    #[allow(dead_code)]
    pub run_started_at: Option<DateTime<Utc>>,
    pub duration_secs: Option<i64>,
}

impl GitHubClient {
    pub async fn rerun_workflow(&self, owner: &str, repo: &str, run_id: u64) -> Result<()> {
        let _: serde_json::Value = self
            .octocrab
            .post(
                format!("/repos/{}/{}/actions/runs/{}/rerun", owner, repo, run_id),
                None::<&()>,
            )
            .await?;
        Ok(())
    }

    /// Fetch recent CI runs for a specific repo.
    pub async fn fetch_repo_ci(&self, owner: &str, repo: &str) -> Result<Vec<WorkflowRun>> {
        let result: serde_json::Value = self
            .octocrab
            .get(
                format!("/repos/{}/{}/actions/runs", owner, repo),
                Some(&[("per_page", "20")]),
            )
            .await?;

        let mut runs = Vec::new();
        if let Some(workflow_runs) = result["workflow_runs"].as_array() {
            for run in workflow_runs {
                let started = run["run_started_at"]
                    .as_str()
                    .and_then(|s| s.parse::<chrono::DateTime<chrono::Utc>>().ok());
                let completed = run["updated_at"]
                    .as_str()
                    .and_then(|s| s.parse::<chrono::DateTime<chrono::Utc>>().ok());

                let duration = match (started, completed) {
                    (Some(s), Some(c)) if run["status"].as_str() == Some("completed") => {
                        Some((c - s).num_seconds())
                    }
                    _ => None,
                };

                runs.push(WorkflowRun {
                    id: run["id"].as_u64().unwrap_or(0),
                    name: run["name"].as_str().unwrap_or("").to_string(),
                    repo_full_name: format!("{}/{}", owner, repo),
                    head_branch: run["head_branch"].as_str().unwrap_or("").to_string(),
                    status: run["status"].as_str().unwrap_or("").to_string(),
                    conclusion: run["conclusion"].as_str().map(String::from),
                    created_at: run["created_at"].as_str().and_then(|s| s.parse().ok()),
                    html_url: run["html_url"].as_str().unwrap_or("").to_string(),
                    run_started_at: started,
                    duration_secs: duration,
                });
            }
        }
        Ok(runs)
    }
}
