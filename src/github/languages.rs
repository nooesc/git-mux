use anyhow::Result;
use super::GitHubClient;

impl GitHubClient {
    /// Fetch language breakdown for a repo.
    /// Returns vec of (language_name, bytes) sorted by bytes descending.
    pub async fn fetch_languages(&self, owner: &str, repo: &str) -> Result<Vec<(String, u64)>> {
        let result: serde_json::Value = self.octocrab.get(
            format!("/repos/{}/{}/languages", owner, repo),
            None::<&()>,
        ).await?;

        let mut langs: Vec<(String, u64)> = result
            .as_object()
            .map(|obj| {
                obj.iter()
                    .map(|(k, v)| (k.clone(), v.as_u64().unwrap_or(0)))
                    .collect()
            })
            .unwrap_or_default();

        langs.sort_by(|a, b| b.1.cmp(&a.1));
        Ok(langs)
    }
}
