use super::GitHubClient;
use anyhow::Result;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContributionDay {
    pub date: NaiveDate,
    pub count: u32,
    pub level: u8, // 0-4, matching GitHub's intensity levels
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ContributionData {
    pub days: Vec<ContributionDay>,
    pub total: u32,
}

impl GitHubClient {
    pub async fn fetch_contributions(&self) -> Result<ContributionData> {
        let query = serde_json::json!({
            "query": format!(r#"
                query {{
                    user(login: "{}") {{
                        contributionsCollection {{
                            contributionCalendar {{
                                totalContributions
                                weeks {{
                                    contributionDays {{
                                        date
                                        contributionCount
                                        contributionLevel
                                    }}
                                }}
                            }}
                        }}
                    }}
                }}
            "#, self.username)
        });

        let response: serde_json::Value = self.octocrab.graphql(&query).await?;

        let calendar = &response["data"]["user"]["contributionsCollection"]["contributionCalendar"];
        let total = calendar["totalContributions"].as_u64().unwrap_or(0) as u32;

        let mut days = Vec::new();
        if let Some(weeks) = calendar["weeks"].as_array() {
            for week in weeks {
                if let Some(week_days) = week["contributionDays"].as_array() {
                    for day in week_days {
                        let date_str = day["date"].as_str().unwrap_or("");
                        if let Ok(date) = date_str.parse::<NaiveDate>() {
                            let level = match day["contributionLevel"].as_str().unwrap_or("NONE") {
                                "NONE" => 0,
                                "FIRST_QUARTILE" => 1,
                                "SECOND_QUARTILE" => 2,
                                "THIRD_QUARTILE" => 3,
                                "FOURTH_QUARTILE" => 4,
                                _ => 0,
                            };
                            days.push(ContributionDay {
                                date,
                                count: day["contributionCount"].as_u64().unwrap_or(0) as u32,
                                level,
                            });
                        }
                    }
                }
            }
        }

        Ok(ContributionData { days, total })
    }
}
