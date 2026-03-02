use super::GitHubClient;
use anyhow::Result;

impl GitHubClient {
    pub async fn fetch_readme(&self, owner: &str, repo: &str) -> Result<String> {
        let result: serde_json::Value = self
            .octocrab
            .get(format!("/repos/{}/{}/readme", owner, repo), None::<&()>)
            .await?;

        let content = result["content"].as_str().unwrap_or("");
        let cleaned: String = content.chars().filter(|c| !c.is_whitespace()).collect();
        use base64::Engine;
        let bytes = base64::engine::general_purpose::STANDARD.decode(&cleaned)?;
        Ok(String::from_utf8(bytes)?)
    }
}
