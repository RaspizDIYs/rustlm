use serde::{Deserialize, Serialize};

use super::rune::RunePage;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AutoAcceptMethod {
    WebSocket,
    Polling,
    UIA,
    Auto,
}

impl Default for AutoAcceptMethod {
    fn default() -> Self {
        Self::Polling
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct AutomationSettings {
    #[serde(default)]
    pub champion_to_pick1: String,
    #[serde(default)]
    pub champion_to_pick2: String,
    #[serde(default)]
    pub champion_to_pick3: String,
    #[serde(default)]
    pub champion_to_ban: String,
    #[serde(default)]
    pub summoner_spell1: String,
    #[serde(default)]
    pub summoner_spell2: String,
    #[serde(default)]
    pub is_enabled: bool,
    #[serde(default = "default_auto_accept_method")]
    pub auto_accept_method: String,
    #[serde(default)]
    pub rune_pages: Vec<RunePage>,
    #[serde(default)]
    pub selected_rune_page_name: String,
    #[serde(default)]
    pub is_pick_delay_enabled: bool,
    #[serde(default)]
    pub pick_delay_seconds: i32,
    #[serde(default)]
    pub auto_rune_generation_enabled: bool,

    // Individual automation toggles
    #[serde(default = "default_true")]
    pub auto_pick_enabled: bool,
    #[serde(default = "default_true")]
    pub auto_ban_enabled: bool,
    #[serde(default = "default_true")]
    pub auto_spells_enabled: bool,
    #[serde(default = "default_true")]
    pub auto_runes_enabled: bool,

    // Runtime IDs resolved from champion/spell names
    #[serde(default)]
    pub pick_champion1: Option<String>,
    #[serde(default)]
    pub pick_champion2: Option<String>,
    #[serde(default)]
    pub pick_champion3: Option<String>,
    #[serde(default)]
    pub ban_champion: Option<String>,
    #[serde(default)]
    pub pick_champion1_id: Option<i32>,
    #[serde(default)]
    pub pick_champion2_id: Option<i32>,
    #[serde(default)]
    pub pick_champion3_id: Option<i32>,
    #[serde(default)]
    pub ban_champion_id: Option<i32>,
    #[serde(default)]
    pub spell1_id: Option<i32>,
    #[serde(default)]
    pub spell2_id: Option<i32>,
}

fn default_true() -> bool {
    true
}

fn default_auto_accept_method() -> String {
    "Polling".to_string()
}

impl Default for AutomationSettings {
    fn default() -> Self {
        Self {
            champion_to_pick1: String::new(),
            champion_to_pick2: String::new(),
            champion_to_pick3: String::new(),
            champion_to_ban: String::new(),
            summoner_spell1: String::new(),
            summoner_spell2: String::new(),
            is_enabled: false,
            auto_accept_method: "Polling".to_string(),
            rune_pages: Vec::new(),
            selected_rune_page_name: String::new(),
            is_pick_delay_enabled: false,
            pick_delay_seconds: 0,
            auto_rune_generation_enabled: false,
            auto_pick_enabled: true,
            auto_ban_enabled: true,
            auto_spells_enabled: true,
            auto_runes_enabled: true,
            pick_champion1: None,
            pick_champion2: None,
            pick_champion3: None,
            ban_champion: None,
            pick_champion1_id: None,
            pick_champion2_id: None,
            pick_champion3_id: None,
            ban_champion_id: None,
            spell1_id: None,
            spell2_id: None,
        }
    }
}
