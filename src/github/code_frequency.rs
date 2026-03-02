use super::GitHubClient;
use anyhow::Result;

impl GitHubClient {
    /// Fetch weekly code frequency (additions/deletions) for a repo.
    /// GitHub stats API returns 202 while computing; retry a few times.
    /// Returns vec of (week_unix_timestamp, additions, deletions).
    pub async fn fetch_code_frequency(
        &self,
        owner: &str,
        repo: &str,
    ) -> Result<Vec<(i64, i64, i64)>> {
        let mut result = serde_json::Value::Null;
        for _ in 0..3 {
            result = self
                .octocrab
                .get(
                    format!("/repos/{}/{}/stats/code_frequency", owner, repo),
                    None::<&()>,
                )
                .await
                .unwrap_or(serde_json::Value::Null);
            if result.is_array() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }

        let weeks: Vec<(i64, i64, i64)> = result
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|item| {
                        let triple = item.as_array()?;
                        if triple.len() >= 3 {
                            Some((
                                triple[0].as_i64().unwrap_or(0),
                                triple[1].as_i64().unwrap_or(0),
                                triple[2].as_i64().unwrap_or(0),
                            ))
                        } else {
                            None
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(weeks)
    }
}
