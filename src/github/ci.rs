use anyhow::Result;
use chrono::{DateTime, Utc};
use super::GitHubClient;

#[derive(Debug, Clone)]
pub struct WorkflowRun {
    pub id: u64,
    pub name: String,
    pub repo_full_name: String,
    pub head_branch: String,
    pub status: String,      // queued, in_progress, completed
    pub conclusion: Option<String>, // success, failure, cancelled, etc.
    pub created_at: Option<DateTime<Utc>>,
    pub html_url: String,
    #[allow(dead_code)]
    pub run_started_at: Option<DateTime<Utc>>,
    pub duration_secs: Option<i64>,
}

impl GitHubClient {
    /// Fetch recent workflow runs for repos pushed to in the last 7 days.
    pub async fn fetch_ci_runs(&self, repos: &[crate::github::repos::RepoInfo]) -> Result<Vec<WorkflowRun>> {
        let mut all_runs = Vec::new();
        let cutoff = Utc::now() - chrono::Duration::days(7);

        let active_repos: Vec<_> = repos
            .iter()
            .filter(|r| r.pushed_at.map(|d| d > cutoff).unwrap_or(false))
            .take(20)
            .collect();

        for repo in active_repos {
            let parts: Vec<&str> = repo.full_name.splitn(2, '/').collect();
            if parts.len() != 2 { continue; }
            let (owner, name) = (parts[0], parts[1]);

            let result: std::result::Result<serde_json::Value, _> = self.octocrab.get(
                format!("/repos/{}/{}/actions/runs", owner, name),
                Some(&[("per_page", "5")]),
            ).await;
            match result {
                Ok(response) => {
                    if let Some(runs) = response["workflow_runs"].as_array() {
                        for run in runs {
                            let started = run["run_started_at"]
                                .as_str()
                                .and_then(|s| s.parse::<DateTime<Utc>>().ok());
                            let completed = run["updated_at"]
                                .as_str()
                                .and_then(|s| s.parse::<DateTime<Utc>>().ok());

                            let duration = match (started, completed) {
                                (Some(s), Some(c)) if run["status"].as_str() == Some("completed") => {
                                    Some((c - s).num_seconds())
                                }
                                _ => None,
                            };

                            all_runs.push(WorkflowRun {
                                id: run["id"].as_u64().unwrap_or(0),
                                name: run["name"].as_str().unwrap_or("").to_string(),
                                repo_full_name: repo.full_name.clone(),
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
                }
                Err(_) => continue,
            }
        }

        all_runs.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(all_runs)
    }

    pub async fn rerun_workflow(&self, owner: &str, repo: &str, run_id: u64) -> Result<()> {
        let _: serde_json::Value = self.octocrab.post(
            format!("/repos/{}/{}/actions/runs/{}/rerun", owner, repo, run_id),
            None::<&()>,
        ).await?;
        Ok(())
    }

    /// Fetch recent CI runs for a specific repo.
    pub async fn fetch_repo_ci(&self, owner: &str, repo: &str) -> Result<Vec<WorkflowRun>> {
        let result: serde_json::Value = self.octocrab.get(
            format!("/repos/{}/{}/actions/runs", owner, repo),
            Some(&[("per_page", "20")]),
        ).await?;

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
