#![allow(dead_code)]

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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
    // --- Champ-select slot data ---
    // Слот в выборе чемпионов. Номер клетки нужен, чтобы показывать всех
    // союзников, даже когда клиент не отдаёт их ники (ранговый выбор).
    #[serde(default)]
    pub cell_id: i32,
    #[serde(default)]
    pub team: i32,
    #[serde(default)]
    pub position: String,
    #[serde(default)]
    pub is_self: bool,
    // true — клиент скрыл ник (Riot анонимизирует союзников в ранговом выборе).
    #[serde(default)]
    pub identity_hidden: bool,
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

/// Снимок разведки для фронтенда: обе команды плюс состояние, по которому UI
/// объясняет, почему часть данных недоступна.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevealSnapshot {
    pub allies: Vec<PlayerInfo>,
    pub enemies: Vec<PlayerInfo>,
    pub in_champ_select: bool,
    pub queue_id: i64,
    pub queue_name: String,
    pub is_ranked: bool,
    pub is_custom: bool,
    // true — хотя бы у одного союзника клиент не отдал ник.
    pub identity_hidden: bool,
    pub api_configured: bool,
    // Ники, которые клиент отдал, но без привязки к слотам (полная анонимизация).
    #[serde(default)]
    pub unmatched_names: Vec<String>,
    #[serde(default)]
    pub api_error: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
}

impl Default for RevealSnapshot {
    fn default() -> Self {
        Self {
            allies: Vec::new(),
            enemies: Vec::new(),
            in_champ_select: false,
            queue_id: 0,
            queue_name: String::new(),
            is_ranked: false,
            is_custom: false,
            identity_hidden: false,
            api_configured: false,
            unmatched_names: Vec::new(),
            api_error: None,
            note: None,
        }
    }
}

/// Одна сыгранная игра аккаунта — компактная строка для раскрывающегося
/// аккаунта в списке.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchSummary {
    pub match_id: String,
    pub champion_id: i32,
    pub champion_name: String,
    pub win: bool,
    pub kills: i32,
    pub deaths: i32,
    pub assists: i32,
    pub queue_name: String,
    pub position: String,
    pub duration_secs: i64,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegionInfo {
    pub code: String,
    pub name: String,
}
