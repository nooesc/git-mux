pub mod auth;
pub mod avatar;
pub mod ci;
pub mod contributions;
pub mod issues;
pub mod notifications;
pub mod prs;
pub mod repos;

use anyhow::Result;
use octocrab::Octocrab;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct UserInfo {
    pub login: String,
    pub avatar_url: String,
    pub public_repos: u32,
    pub followers: u32,
}

pub struct GitHubClient {
    pub octocrab: Octocrab,
    pub username: String,
    pub user_info: UserInfo,
}

impl GitHubClient {
    pub async fn new() -> Result<Arc<Self>> {
        let token = auth::get_token()?;
        let octocrab = Octocrab::builder()
            .personal_token(token)
            .build()?;

        let user: serde_json::Value = octocrab.get("/user", None::<&()>).await?;
        let username = user["login"]
            .as_str()
            .unwrap_or("unknown")
            .to_string();

        let user_info = UserInfo {
            login: username.clone(),
            avatar_url: user["avatar_url"].as_str().unwrap_or("").to_string(),
            public_repos: user["public_repos"].as_u64().unwrap_or(0) as u32,
            followers: user["followers"].as_u64().unwrap_or(0) as u32,
        };

        Ok(Arc::new(Self { octocrab, username, user_info }))
    }

    /// Create a lightweight client that skips the /user API call.
    /// Use this for action handlers where user info is already known.
    pub fn from_token() -> Result<Arc<Self>> {
        let token = auth::get_token()?;
        let octocrab = Octocrab::builder()
            .personal_token(token)
            .build()?;

        Ok(Arc::new(Self {
            octocrab,
            username: String::new(),
            user_info: UserInfo {
                login: String::new(),
                avatar_url: String::new(),
                public_repos: 0,
                followers: 0,
            },
        }))
    }
}
