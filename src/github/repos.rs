use super::GitHubClient;
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoInfo {
    pub full_name: String,
    pub description: Option<String>,
    pub language: Option<String>,
    pub stargazers_count: u32,
    pub forks_count: u32,
    pub open_issues_count: u32,
    pub pushed_at: Option<DateTime<Utc>>,
    pub html_url: String,
    #[allow(dead_code)]
    pub is_fork: bool,
    pub is_private: bool,
    #[allow(dead_code)]
    pub owner: String,
    #[serde(default)]
    pub open_issues_only_count: Option<u32>,
    #[serde(default)]
    pub open_prs_count: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoOpenCounts {
    pub full_name: String,
    pub open_issues_count: u32,
    pub open_prs_count: u32,
}

impl GitHubClient {
    /// Fetch all repos for the authenticated user (personal + all orgs).
    /// Fetches personal repos and org list concurrently, then all org repos in parallel.
    pub async fn fetch_all_repos(&self) -> Result<Vec<RepoInfo>> {
        // Step 1: Fetch personal repos and org list in parallel
        let (personal_result, orgs_result) =
            tokio::join!(self.fetch_paginated_repos("/user/repos"), async {
                let orgs: Vec<serde_json::Value> =
                    self.octocrab.get("/user/orgs", None::<&()>).await?;
                Ok::<_, anyhow::Error>(orgs)
            });

        let mut all_repos = personal_result?;
        let orgs = orgs_result?;

        // Step 2: Fetch all org repos in parallel
        let org_logins: Vec<String> = orgs
            .iter()
            .filter_map(|o| o["login"].as_str().map(String::from))
            .collect();

        let org_futures: Vec<_> = org_logins
            .iter()
            .map(|login| self.fetch_paginated_repos(format!("/orgs/{}/repos", login)))
            .collect();

        let org_results = futures::future::join_all(org_futures).await;
        for result in org_results {
            match result {
                Ok(repos) => all_repos.extend(repos),
                Err(_) => continue, // skip orgs we can't access
            }
        }

        // Deduplicate by full_name (personal repos can overlap with org repos)
        all_repos.sort_by(|a, b| a.full_name.cmp(&b.full_name));
        all_repos.dedup_by(|a, b| a.full_name == b.full_name);

        // Sort by most recently pushed
        all_repos.sort_by(|a, b| b.pushed_at.cmp(&a.pushed_at));

        Ok(all_repos)
    }

    /// Fetch repos from a paginated endpoint.
    async fn fetch_paginated_repos(&self, endpoint: impl Into<String>) -> Result<Vec<RepoInfo>> {
        let endpoint = endpoint.into();
        let mut repos = Vec::new();
        let mut page = 1u32;
        loop {
            let batch: Vec<serde_json::Value> = self
                .octocrab
                .get(
                    &endpoint,
                    Some(&[
                        ("per_page", "100"),
                        ("page", &page.to_string()),
                        ("sort", "pushed"),
                    ]),
                )
                .await?;

            if batch.is_empty() {
                break;
            }

            let done = batch.len() < 100;
            repos.extend(batch.iter().map(parse_repo));

            if done {
                break;
            }
            page += 1;
        }
        Ok(repos)
    }

    /// Fetch open issue/PR counts for repos in batches via GraphQL.
    pub async fn fetch_repo_open_counts(&self, repos: &[RepoInfo]) -> Result<Vec<RepoOpenCounts>> {
        if repos.is_empty() {
            return Ok(Vec::new());
        }

        let mut out = Vec::new();
        for chunk in repos.chunks(20) {
            let mut query = String::from("query {");
            let mut aliases: Vec<(String, String)> = Vec::new();

            for (idx, repo) in chunk.iter().enumerate() {
                let mut parts = repo.full_name.splitn(2, '/');
                let owner = parts.next().unwrap_or("");
                let name = parts.next().unwrap_or("");
                if owner.is_empty() || name.is_empty() {
                    continue;
                }

                let alias = format!("r{}", idx);
                let owner_json = serde_json::to_string(owner)?;
                let name_json = serde_json::to_string(name)?;
                query.push_str(&format!(
                    "{alias}: repository(owner:{owner_json}, name:{name_json}) {{ issues(states:OPEN) {{ totalCount }} pullRequests(states:OPEN) {{ totalCount }} }}"
                ));
                aliases.push((alias, repo.full_name.clone()));
            }
            query.push('}');

            let data: serde_json::Value = self
                .octocrab
                .graphql(&serde_json::json!({ "query": query }))
                .await?;
            for (alias, full_name) in aliases {
                let node = &data[alias];
                if node.is_null() {
                    continue;
                }
                let issues = node["issues"]["totalCount"].as_u64().unwrap_or(0) as u32;
                let prs = node["pullRequests"]["totalCount"].as_u64().unwrap_or(0) as u32;
                out.push(RepoOpenCounts {
                    full_name,
                    open_issues_count: issues,
                    open_prs_count: prs,
                });
            }
        }

        Ok(out)
    }
}

fn parse_repo(json: &serde_json::Value) -> RepoInfo {
    RepoInfo {
        full_name: json["full_name"].as_str().unwrap_or("").to_string(),
        description: json["description"].as_str().map(|s| s.to_string()),
        language: json["language"].as_str().map(|s| s.to_string()),
        stargazers_count: json["stargazers_count"].as_u64().unwrap_or(0) as u32,
        forks_count: json["forks_count"].as_u64().unwrap_or(0) as u32,
        open_issues_count: json["open_issues_count"].as_u64().unwrap_or(0) as u32,
        pushed_at: json["pushed_at"]
            .as_str()
            .and_then(|s| s.parse::<DateTime<Utc>>().ok()),
        html_url: json["html_url"].as_str().unwrap_or("").to_string(),
        is_fork: json["fork"].as_bool().unwrap_or(false),
        is_private: json["private"].as_bool().unwrap_or(false),
        owner: json["owner"]["login"].as_str().unwrap_or("").to_string(),
        open_issues_only_count: None,
        open_prs_count: None,
    }
}
