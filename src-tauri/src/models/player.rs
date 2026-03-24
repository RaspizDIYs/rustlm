#![allow(dead_code)]

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerInfo {
    #[serde(default)]
    pub riot_id: String,
    #[serde(default)]
    pub summoner_name: String,
    #[serde(default)]
    pub champion_id: i32,
    #[serde(default = "default_rank")]
    pub rank: String,
    #[serde(default)]
    pub tier: String,
    #[serde(default)]
    pub league_points: i32,
    #[serde(default)]
    pub wins: i32,
    #[serde(default)]
    pub losses: i32,
    #[serde(default = "default_win_rate")]
    pub win_rate: String,
    #[serde(default)]
    pub level: i32,
    #[serde(default)]
    pub profile_icon_id: i32,
    #[serde(default)]
    pub puuid: String,
    #[serde(default)]
    pub ugg_link: String,
}

fn default_rank() -> String {
    "Unranked".to_string()
}
fn default_win_rate() -> String {
    "0%".to_string()
}

impl PlayerInfo {
    pub fn full_rank(&self) -> String {
        if self.tier.is_empty() || self.tier == "Unranked" {
            "Unranked".to_string()
        } else {
            format!("{} {} {}LP", self.tier, self.rank, self.league_points)
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegionInfo {
    pub code: String,
    pub name: String,
}
