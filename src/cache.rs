use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::github::UserInfo;
use crate::github::ci::WorkflowRun;
use crate::github::commits::{CommitInfo, WeeklyCommitActivity};
use crate::github::contributions::ContributionData;
use crate::github::contributors::ContributorInfo;
use crate::github::issues::IssueInfo;
use crate::github::notifications::Notification;
use crate::github::prs::PrInfo;
use crate::github::repos::RepoInfo;

const CACHE_SCHEMA_VERSION: u8 = 1;
const MAX_STARTUP_CACHE_AGE_HOURS: i64 = 24 * 7;
const REPO_DETAIL_CACHE_SCHEMA_VERSION: u8 = 1;
const MAX_REPO_DETAIL_CACHE_AGE_HOURS: i64 = 24 * 7;
const REPO_DETAIL_FRESH_MINUTES: i64 = 15;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartupCache {
    pub schema_version: u8,
    pub saved_at: DateTime<Utc>,
    pub user_info: Option<UserInfo>,
    pub repos: Option<Vec<RepoInfo>>,
    pub contributions: Option<ContributionData>,
    pub notifications: Option<Vec<Notification>>,
    pub avatar_png: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoDetailCache {
    pub schema_version: u8,
    pub saved_at: DateTime<Utc>,
    pub repo_full_name: String,
    pub prs: Vec<PrInfo>,
    pub issues: Vec<IssueInfo>,
    pub ci: Vec<WorkflowRun>,
    pub commits: Vec<CommitInfo>,
    pub commit_activity: Vec<WeeklyCommitActivity>,
    pub readme: Option<String>,
    pub languages: Vec<(String, u64)>,
    pub contributors: Vec<ContributorInfo>,
    pub code_frequency: Vec<(i64, i64, i64)>,
}

impl RepoDetailCache {
    pub fn new(repo_full_name: String) -> Self {
        Self {
            schema_version: REPO_DETAIL_CACHE_SCHEMA_VERSION,
            saved_at: Utc::now(),
            repo_full_name,
            prs: Vec::new(),
            issues: Vec::new(),
            ci: Vec::new(),
            commits: Vec::new(),
            commit_activity: Vec::new(),
            readme: None,
            languages: Vec::new(),
            contributors: Vec::new(),
            code_frequency: Vec::new(),
        }
    }
}

impl Default for StartupCache {
    fn default() -> Self {
        Self {
            schema_version: CACHE_SCHEMA_VERSION,
            saved_at: Utc::now(),
            user_info: None,
            repos: None,
            contributions: None,
            notifications: None,
            avatar_png: None,
        }
    }
}

impl StartupCache {
    pub fn fresh_from_previous(previous: Option<Self>) -> Self {
        let mut next = previous.unwrap_or_default();
        next.schema_version = CACHE_SCHEMA_VERSION;
        next.saved_at = Utc::now();
        next
    }
}

pub fn load_startup_cache() -> Option<StartupCache> {
    let path = cache_path();
    let raw = std::fs::read_to_string(path).ok()?;
    let cache: StartupCache = serde_json::from_str(&raw).ok()?;

    if cache.schema_version != CACHE_SCHEMA_VERSION {
        return None;
    }

    let age = Utc::now().signed_duration_since(cache.saved_at);
    if age > Duration::hours(MAX_STARTUP_CACHE_AGE_HOURS) {
        return None;
    }

    Some(cache)
}

pub fn load_repo_detail_cache(repo_full_name: &str) -> Option<RepoDetailCache> {
    let path = repo_detail_cache_path(repo_full_name);
    let raw = std::fs::read_to_string(path).ok()?;
    let cache: RepoDetailCache = serde_json::from_str(&raw).ok()?;

    if cache.schema_version != REPO_DETAIL_CACHE_SCHEMA_VERSION {
        return None;
    }
    if cache.repo_full_name != repo_full_name {
        return None;
    }

    let age = Utc::now().signed_duration_since(cache.saved_at);
    if age > Duration::hours(MAX_REPO_DETAIL_CACHE_AGE_HOURS) {
        return None;
    }

    Some(cache)
}

pub fn repo_detail_cache_is_fresh(saved_at: DateTime<Utc>) -> bool {
    Utc::now().signed_duration_since(saved_at) <= Duration::minutes(REPO_DETAIL_FRESH_MINUTES)
}

pub fn save_startup_cache(cache: &StartupCache) -> Result<()> {
    let path = cache_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create cache directory {}", parent.display()))?;
    }

    let payload = serde_json::to_vec_pretty(cache)?;
    let tmp_path = tmp_cache_path();
    std::fs::write(&tmp_path, payload)
        .with_context(|| format!("failed to write cache temp file {}", tmp_path.display()))?;
    std::fs::rename(&tmp_path, &path)
        .with_context(|| format!("failed to atomically replace cache file {}", path.display()))?;

    Ok(())
}

pub fn save_repo_detail_cache(cache: &RepoDetailCache) -> Result<()> {
    let path = repo_detail_cache_path(&cache.repo_full_name);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create cache directory {}", parent.display()))?;
    }

    let payload = serde_json::to_vec_pretty(cache)?;
    let tmp_path = repo_detail_tmp_cache_path(&cache.repo_full_name);
    std::fs::write(&tmp_path, payload)
        .with_context(|| format!("failed to write cache temp file {}", tmp_path.display()))?;
    std::fs::rename(&tmp_path, &path)
        .with_context(|| format!("failed to atomically replace cache file {}", path.display()))?;

    Ok(())
}

fn cache_path() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("git-mux")
        .join("startup_cache.json")
}

fn tmp_cache_path() -> PathBuf {
    let nanos = Utc::now().timestamp_nanos_opt().unwrap_or_default();
    dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("git-mux")
        .join(format!("startup_cache.{}.tmp", nanos))
}

fn repo_detail_cache_path(repo_full_name: &str) -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("git-mux")
        .join("repo_detail")
        .join(format!("{}.json", sanitize_repo_cache_key(repo_full_name)))
}

fn repo_detail_tmp_cache_path(repo_full_name: &str) -> PathBuf {
    let nanos = Utc::now().timestamp_nanos_opt().unwrap_or_default();
    dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("git-mux")
        .join("repo_detail")
        .join(format!(
            "{}.{}.tmp",
            sanitize_repo_cache_key(repo_full_name),
            nanos
        ))
}

fn sanitize_repo_cache_key(repo_full_name: &str) -> String {
    repo_full_name
        .replace('/', "__")
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}
