use anyhow::Result;
use super::GitHubClient;

#[derive(Debug, Clone)]
pub struct ContributorInfo {
    pub login: String,
    pub total_commits: u32,
}

impl GitHubClient {
    /// Fetch contributor stats for a repo.
    /// GitHub stats API returns 202 while computing; retry a few times.
    /// Returns vec of ContributorInfo sorted by total_commits descending.
    pub async fn fetch_contributors(&self, owner: &str, repo: &str) -> Result<Vec<ContributorInfo>> {
        let mut result = serde_json::Value::Null;
        for _ in 0..3 {
            result = self.octocrab.get(
                format!("/repos/{}/{}/stats/contributors", owner, repo),
                None::<&()>,
            ).await.unwrap_or(serde_json::Value::Null);
            if result.is_array() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }

        let mut contributors: Vec<ContributorInfo> = result
            .as_array()
            .map(|arr| {
                arr.iter().map(|item| {
                    ContributorInfo {
                        login: item["author"]["login"].as_str().unwrap_or("unknown").to_string(),
                        total_commits: item["total"].as_u64().unwrap_or(0) as u32,
                    }
                }).collect()
            })
            .unwrap_or_default();

        contributors.sort_by(|a, b| b.total_commits.cmp(&a.total_commits));
        Ok(contributors)
    }
}
