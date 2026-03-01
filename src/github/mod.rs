pub mod auth;
pub mod contributions;
pub mod notifications;
pub mod prs;
pub mod repos;

use anyhow::Result;
use octocrab::Octocrab;

pub struct GitHubClient {
    pub octocrab: Octocrab,
    pub username: String,
}

impl GitHubClient {
    pub async fn new() -> Result<Self> {
        let token = auth::get_token()?;
        let octocrab = Octocrab::builder()
            .personal_token(token)
            .build()?;

        // Fetch authenticated user's login
        let user: serde_json::Value = octocrab.get("/user", None::<&()>).await?;
        let username = user["login"]
            .as_str()
            .unwrap_or("unknown")
            .to_string();

        Ok(Self { octocrab, username })
    }
}
