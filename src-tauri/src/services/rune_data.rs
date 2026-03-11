use std::collections::HashMap;
use tokio::sync::{Mutex, RwLock};

use crate::error::AppError;
use crate::models::rune::{Rune, RunePath, RuneSlot};

const DDRAGON_BASE: &str = "https://ddragon.leagueoflegends.com";
const CDRAGON_PERKS: &str = "https://raw.communitydragon.org/latest/plugins/rcp-be-lol-game-data/global/default/v1/perks.json";

pub struct RuneDataService {
    client: reqwest::Client,
    cached_paths: RwLock<Option<Vec<RunePath>>>,
    all_perks_by_id: RwLock<Option<HashMap<i32, Rune>>>,
    load_lock: Mutex<()>,
}

impl RuneDataService {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap(),
            cached_paths: RwLock::new(None),
            all_perks_by_id: RwLock::new(None),
            load_lock: Mutex::new(()),
        }
    }

    fn stat_mods() -> Vec<Rune> {
        vec![
            // Row 1
            Rune { id: 5008, key: "StatModsAdaptiveForceIcon".into(), name: "Adaptive Force".into(), icon: "perk-images/StatMods/StatModsAdaptiveForceIcon.png".into(), short_desc: "+9 Adaptive Force".into(), long_desc: String::new() },
            Rune { id: 5005, key: "StatModsAttackSpeedIcon".into(), name: "Attack Speed".into(), icon: "perk-images/StatMods/StatModsAttackSpeedIcon.png".into(), short_desc: "+10% Attack Speed".into(), long_desc: String::new() },
            Rune { id: 5007, key: "StatModsCDRScalingIcon".into(), name: "Ability Haste".into(), icon: "perk-images/StatMods/StatModsCDRScalingIcon.png".into(), short_desc: "+8 Ability Haste".into(), long_desc: String::new() },
            // Row 2
            Rune { id: 5008, key: "StatModsAdaptiveForceIcon".into(), name: "Adaptive Force".into(), icon: "perk-images/StatMods/StatModsAdaptiveForceIcon.png".into(), short_desc: "+9 Adaptive Force".into(), long_desc: String::new() },
            Rune { id: 5010, key: "StatModsMovementSpeedIcon".into(), name: "Move Speed".into(), icon: "perk-images/StatMods/StatModsMovementSpeedIcon.png".into(), short_desc: "+2% Move Speed".into(), long_desc: String::new() },
            Rune { id: 5001, key: "StatModsHealthScalingIcon".into(), name: "Health Scaling".into(), icon: "perk-images/StatMods/StatModsHealthScalingIcon.png".into(), short_desc: "+10-180 Health (based on level)".into(), long_desc: String::new() },
            // Row 3
            Rune { id: 5011, key: "StatModsHealthPlusIcon".into(), name: "Health".into(), icon: "perk-images/StatMods/StatModsHealthPlusIcon.png".into(), short_desc: "+65 Health".into(), long_desc: String::new() },
            Rune { id: 5013, key: "StatModsTenacityIcon".into(), name: "Tenacity".into(), icon: "perk-images/StatMods/StatModsTenacityIcon.png".into(), short_desc: "+10% Tenacity and Slow Resist".into(), long_desc: String::new() },
            Rune { id: 5001, key: "StatModsHealthScalingIcon".into(), name: "Health Scaling".into(), icon: "perk-images/StatMods/StatModsHealthScalingIcon.png".into(), short_desc: "+10-180 Health (based on level)".into(), long_desc: String::new() },
        ]
    }

    pub fn get_stat_mods_row1() -> Vec<Rune> {
        Self::stat_mods()[0..3].to_vec()
    }

    pub fn get_stat_mods_row2() -> Vec<Rune> {
        Self::stat_mods()[3..6].to_vec()
    }

    pub fn get_stat_mods_row3() -> Vec<Rune> {
        Self::stat_mods()[6..9].to_vec()
    }

    async fn ensure_paths_loaded(&self) -> Result<(), AppError> {
        {
            let paths = self.cached_paths.read().await;
            if paths.is_some() {
                return Ok(());
            }
        }

        let _lock = self.load_lock.lock().await;

        {
            let paths = self.cached_paths.read().await;
            if paths.is_some() {
                return Ok(());
            }
        }

        // Fetch version
        let versions_url = format!("{}/api/versions.json", DDRAGON_BASE);
        let versions: Vec<String> = self.client.get(&versions_url).send().await?.json().await?;
        let version = versions.first().cloned().unwrap_or_default();

        // Fetch runes
        let url = format!(
            "{}/cdn/{}/data/ru_RU/runesReforged.json",
            DDRAGON_BASE, version
        );
        let raw_paths: Vec<serde_json::Value> = self.client.get(&url).send().await?.json().await?;

        let mut paths = Vec::new();
        for raw in raw_paths {
            let id = raw.get("id").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
            let key = raw.get("key").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let name = raw.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let icon = raw.get("icon").and_then(|v| v.as_str()).unwrap_or("").to_string();

            let mut slots = Vec::new();
            if let Some(slot_arr) = raw.get("slots").and_then(|s| s.as_array()) {
                for slot_val in slot_arr {
                    let mut runes = Vec::new();
                    if let Some(rune_arr) = slot_val.get("runes").and_then(|r| r.as_array()) {
                        for rune_val in rune_arr {
                            runes.push(Rune {
                                id: rune_val.get("id").and_then(|v| v.as_i64()).unwrap_or(0) as i32,
                                key: rune_val.get("key").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                                name: rune_val.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                                icon: rune_val.get("icon").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                                short_desc: rune_val.get("shortDesc").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                                long_desc: rune_val.get("longDesc").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                            });
                        }
                    }
                    slots.push(RuneSlot { runes });
                }
            }

            paths.push(RunePath { id, key, name, icon, slots });
        }

        *self.cached_paths.write().await = Some(paths);
        Ok(())
    }

    async fn ensure_all_perks_loaded(&self) -> Result<(), AppError> {
        {
            let perks = self.all_perks_by_id.read().await;
            if perks.is_some() {
                return Ok(());
            }
        }

        let raw: Vec<serde_json::Value> = self.client.get(CDRAGON_PERKS).send().await?.json().await?;
        let mut map = HashMap::new();
        for perk in raw {
            let id = perk.get("id").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
            let name = perk.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let icon_path = perk.get("iconPath").and_then(|v| v.as_str()).unwrap_or("").to_string();
            map.insert(id, Rune {
                id,
                key: String::new(),
                name,
                icon: icon_path,
                short_desc: String::new(),
                long_desc: String::new(),
            });
        }

        *self.all_perks_by_id.write().await = Some(map);
        Ok(())
    }

    pub async fn get_all_paths(&self) -> Result<Vec<RunePath>, AppError> {
        self.ensure_paths_loaded().await?;
        let paths = self.cached_paths.read().await;
        Ok(paths.as_ref().cloned().unwrap_or_default())
    }

    pub async fn get_path_by_id(&self, id: i32) -> Result<Option<RunePath>, AppError> {
        self.ensure_paths_loaded().await?;
        let paths = self.cached_paths.read().await;
        Ok(paths
            .as_ref()
            .and_then(|p| p.iter().find(|path| path.id == id).cloned()))
    }

    pub async fn get_rune_by_id(&self, id: i32) -> Result<Option<Rune>, AppError> {
        // Check stat mods first
        for rune in Self::stat_mods() {
            if rune.id == id {
                return Ok(Some(rune));
            }
        }

        // Check rune paths
        self.ensure_paths_loaded().await?;
        {
            let paths = self.cached_paths.read().await;
            if let Some(paths) = paths.as_ref() {
                for path in paths {
                    for slot in &path.slots {
                        for rune in &slot.runes {
                            if rune.id == id {
                                return Ok(Some(rune.clone()));
                            }
                        }
                    }
                }
            }
        }

        // Fallback to community dragon
        let _ = self.ensure_all_perks_loaded().await;
        let perks = self.all_perks_by_id.read().await;
        Ok(perks.as_ref().and_then(|p| p.get(&id).cloned()))
    }
}
