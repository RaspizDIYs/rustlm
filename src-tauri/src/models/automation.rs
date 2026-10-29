#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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

    #[serde(default)]
    pub use_role_profiles: bool,
    #[serde(default)]
    pub profiles: BTreeMap<String, AutomationProfile>,

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
    "WebSocket".to_string()
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
            auto_accept_method: "WebSocket".to_string(),
            rune_pages: Vec::new(),
            selected_rune_page_name: String::new(),
            is_pick_delay_enabled: false,
            pick_delay_seconds: 0,
            auto_rune_generation_enabled: false,
            auto_pick_enabled: true,
            auto_ban_enabled: true,
            auto_spells_enabled: true,
            auto_runes_enabled: true,
            use_role_profiles: false,
            profiles: BTreeMap::new(),
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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
pub struct AutomationProfile {
    pub picks: Vec<String>,
    pub pick_ids: Vec<i32>,
    pub ban: String,
    pub ban_id: Option<i32>,
    pub spell1_id: Option<i32>,
    pub spell2_id: Option<i32>,
    pub rune_page: String,
    pub auto_pick: bool,
    pub auto_ban: bool,
    pub auto_spells: bool,
    pub auto_runes: bool,
    pub bench: bool,
    pub arena_pick: String,
}

impl AutomationSettings {
    pub fn for_queue(&self, mode: &str, role: &str) -> Self {
        let mut result = self.clone();
        let key = if mode == "classic" && self.use_role_profiles {
            format!("classic:{}", role)
        } else {
            mode.to_owned()
        };
        if let Some(profile) = self.profiles.get(&key) {
            result.pick_champion1 = profile.picks.get(0).cloned();
            result.pick_champion2 = profile.picks.get(1).cloned();
            result.pick_champion3 = profile.picks.get(2).cloned();
            result.pick_champion1_id = profile.pick_ids.get(0).copied().filter(|id| *id > 0);
            result.pick_champion2_id = profile.pick_ids.get(1).copied().filter(|id| *id > 0);
            result.pick_champion3_id = profile.pick_ids.get(2).copied().filter(|id| *id > 0);
            result.ban_champion = Some(profile.ban.clone());
            result.ban_champion_id = profile.ban_id;
            result.spell1_id = profile.spell1_id;
            result.spell2_id = profile.spell2_id;
            result.selected_rune_page_name = profile.rune_page.clone();
            result.auto_pick_enabled = profile.auto_pick;
            result.auto_ban_enabled = profile.auto_ban;
            result.auto_spells_enabled = profile.auto_spells;
            result.auto_runes_enabled = profile.auto_runes;
        } else if mode != "classic" || self.use_role_profiles {
            result.auto_pick_enabled = false;
            result.auto_ban_enabled = false;
            result.auto_spells_enabled = false;
            result.auto_runes_enabled = false;
        }
        if mode == "aram" || mode == "mayhem" {
            result.auto_ban_enabled = false;
        }
        if mode == "mayhem" || mode == "arena" {
            result.auto_runes_enabled = false;
        }
        if mode == "arena" {
            result.auto_spells_enabled = false;
        }
        result
    }
}
