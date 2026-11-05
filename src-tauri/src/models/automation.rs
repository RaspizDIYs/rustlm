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
    pub ban_champion_ids: Vec<i32>,
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
            ban_champion_ids: Vec::new(),
            spell1_id: None,
            spell2_id: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
pub struct AutomationProfile {
    pub picks: Vec<String>,
    pub pick_count: Option<usize>,
    pub pick_ids: Vec<i32>,
    pub bans: Vec<String>,
    pub ban_ids: Vec<i32>,
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
        let key = if mode == "classic" {
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
            result.ban_champion = profile
                .bans
                .first()
                .cloned()
                .or_else(|| Some(profile.ban.clone()));
            result.ban_champion_ids = profile.active_ban_ids();
            result.ban_champion_id = if profile.bans.is_empty() {
                profile.ban_id
            } else {
                profile.ban_ids.first().copied().filter(|id| *id > 0)
            };
            result.spell1_id = profile.spell1_id;
            result.spell2_id = profile.spell2_id;
            result.selected_rune_page_name = profile.rune_page.clone();
            result.auto_pick_enabled = if mode == "classic" {
                self.auto_pick_enabled
            } else {
                profile.auto_pick
            };
            result.auto_ban_enabled = if mode == "classic" {
                self.auto_ban_enabled
            } else {
                profile.auto_ban
            };
            result.auto_spells_enabled = profile.auto_spells;
            result.auto_runes_enabled = profile.auto_runes;
        } else {
            result.auto_pick_enabled = false;
            result.auto_ban_enabled = false;
            result.auto_spells_enabled = false;
            result.auto_runes_enabled = false;
        }
        if mode == "classic" && role == "jungle" {
            if result.spell2_id != Some(11) || result.spell1_id == Some(11) {
                let other = result
                    .spell2_id
                    .filter(|id| *id != 11)
                    .or(result.spell1_id.filter(|id| *id != 11));
                result.spell1_id = Some(11);
                result.spell2_id = other;
            }
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

impl AutomationProfile {
    pub fn active_ban_ids(&self) -> Vec<i32> {
        let ids = if self.bans.is_empty() {
            self.ban_id.into_iter().collect::<Vec<_>>()
        } else {
            self.ban_ids.iter().take(3).copied().collect()
        };
        let mut seen = std::collections::BTreeSet::new();
        ids.into_iter()
            .filter(|id| *id > 0 && !self.pick_ids.contains(id) && seen.insert(*id))
            .collect()
    }

    pub fn normalize_bans(&mut self) -> Result<(), String> {
        if self.bans.len() > 3 || self.bans.iter().any(|name| name.len() > 100) {
            return Err("Некорректный пул банов (максимум 3)".into());
        }
        if self.bans.is_empty() && !self.ban.trim().is_empty() {
            self.bans.push(self.ban.clone());
            self.ban_ids = vec![self.ban_id.unwrap_or(0)];
        }
        let mut seen = std::collections::BTreeSet::new();
        let mut ids = Vec::new();
        let old_ids = self.ban_ids.clone();
        let mut index = 0;
        self.bans.retain_mut(|name| {
            let id = old_ids.get(index).copied().unwrap_or(0);
            index += 1;
            *name = name.trim().to_owned();
            if name.is_empty() || !seen.insert(name.to_lowercase()) {
                return false;
            }
            ids.push(id);
            true
        });
        self.ban_ids = ids;
        self.ban = self.bans.first().cloned().unwrap_or_default();
        self.ban_id = self.ban_ids.first().copied().filter(|id| *id > 0);
        Ok(())
    }

    pub fn active_pick_ids(&self) -> &[i32] {
        &self.pick_ids[..self
            .pick_count
            .unwrap_or(self.pick_ids.len())
            .min(self.pick_ids.len())]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_ban_migrates_and_populated_pool_wins() {
        let mut profile: AutomationProfile =
            serde_json::from_str(r#"{"Ban":"Lux","BanId":99}"#).unwrap();
        assert_eq!(profile.active_ban_ids(), vec![99]);
        profile.normalize_bans().unwrap();
        assert_eq!(profile.bans, vec!["Lux"]);
        assert_eq!(profile.ban_ids, vec![99]);
        profile.bans = vec!["Annie".into(), "Olaf".into(), "Galio".into()];
        profile.ban_ids = vec![1, 2, 3];
        profile.normalize_bans().unwrap();
        assert_eq!(profile.ban, "Annie");
        assert_eq!(profile.ban_id, Some(1));
        let mut settings = AutomationSettings::default();
        settings
            .profiles
            .insert("classic:top".into(), profile.clone());
        settings.profiles.insert("arena".into(), profile);
        for (mode, role) in [("classic", "top"), ("arena", "")] {
            let effective = settings.for_queue(mode, role);
            assert_eq!(effective.ban_champion_ids, vec![1, 2, 3]);
            assert_eq!(effective.ban_champion_id, Some(1));
        }
    }

    #[test]
    fn own_configured_picks_are_never_ban_candidates_including_legacy_single_ban() {
        let mut profile = AutomationProfile {
            pick_ids: vec![99, 2],
            pick_count: Some(1),
            bans: vec!["Lux".into(), "Annie".into(), "Olaf".into()],
            ban_ids: vec![99, 1, 2],
            ..Default::default()
        };
        assert_eq!(profile.active_ban_ids(), vec![1]);
        profile.bans.clear();
        profile.ban = "Lux".into();
        profile.ban_id = Some(99);
        assert!(profile.active_ban_ids().is_empty());
        profile.normalize_bans().unwrap();
        assert!(profile.active_ban_ids().is_empty());
    }

    #[test]
    fn bans_validate_limit_and_deduplicate_in_priority_order() {
        let mut p = AutomationProfile {
            bans: vec!["Lux".into(), " lux ".into(), "Annie".into()],
            ban_ids: vec![99, 99, 1],
            ..Default::default()
        };
        p.normalize_bans().unwrap();
        assert_eq!(p.bans, vec!["Lux", "Annie"]);
        assert_eq!(p.active_ban_ids(), vec![99, 1]);
        p.bans = vec!["Lux".into(); 4];
        assert!(p.normalize_bans().is_err());
        p.bans = vec!["x".repeat(101)];
        assert!(p.normalize_bans().is_err());
    }

    #[test]
    fn classic_switches_apply_to_every_configured_role() {
        let mut settings = AutomationSettings::default();
        for role in ["top", "jungle", "middle", "bottom", "utility"] {
            settings.profiles.insert(
                format!("classic:{role}"),
                AutomationProfile {
                    auto_pick: true,
                    auto_ban: true,
                    ..Default::default()
                },
            );
        }
        settings.auto_pick_enabled = false;
        settings.auto_ban_enabled = false;
        for role in ["top", "jungle", "middle", "bottom", "utility"] {
            let profile = settings.for_queue("classic", role);
            assert!(!profile.auto_pick_enabled);
            assert!(!profile.auto_ban_enabled);
        }
        settings.auto_pick_enabled = true;
        assert!(settings.for_queue("classic", "top").auto_pick_enabled);
    }
    #[test]
    fn jungle_has_exactly_one_smite_and_keeps_the_other_spell() {
        for (first, second, expected) in [
            (Some(4), Some(11), (Some(4), Some(11))),
            (Some(4), Some(14), (Some(11), Some(14))),
            (Some(11), Some(11), (Some(11), None)),
        ] {
            let mut settings = AutomationSettings::default();
            settings.profiles.insert(
                "classic:jungle".into(),
                AutomationProfile {
                    spell1_id: first,
                    spell2_id: second,
                    ..Default::default()
                },
            );
            let result = settings.for_queue("classic", "jungle");
            assert_eq!((result.spell1_id, result.spell2_id), expected);
        }
    }
    #[test]
    fn random_priorities_include_thirtieth_pick_and_respect_visible_count() {
        let mut profile = AutomationProfile {
            pick_ids: (1..=30).collect(),
            pick_count: Some(30),
            ..Default::default()
        };
        assert_eq!(profile.active_pick_ids().last(), Some(&30));
        profile.pick_count = Some(3);
        assert_eq!(profile.active_pick_ids(), &[1, 2, 3]);
        assert_eq!(profile.pick_ids.len(), 30);
    }
}
