use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};

use crate::error::AppError;
use crate::models::champion::{ChampionInfo, SkinInfo};

const DDRAGON_BASE: &str = "https://ddragon.leagueoflegends.com";

pub struct DataDragonService {
    client: reqwest::Client,
    version: RwLock<Option<String>>,
    champions: RwLock<Option<HashMap<String, String>>>,
    champion_info_cache: RwLock<HashMap<String, ChampionInfo>>,
    spells: RwLock<Option<HashMap<String, String>>>,
    version_lock: Mutex<()>,
    champion_lock: Mutex<()>,
    spell_lock: Mutex<()>,
}

impl DataDragonService {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap(),
            version: RwLock::new(None),
            champions: RwLock::new(None),
            champion_info_cache: RwLock::new(HashMap::new()),
            spells: RwLock::new(None),
            version_lock: Mutex::new(()),
            champion_lock: Mutex::new(()),
            spell_lock: Mutex::new(()),
        }
    }

    pub async fn get_latest_version(&self) -> Result<String, AppError> {
        {
            let ver = self.version.read().await;
            if let Some(v) = ver.as_ref() {
                return Ok(v.clone());
            }
        }

        let _lock = self.version_lock.lock().await;

        // Double-check after acquiring lock
        {
            let ver = self.version.read().await;
            if let Some(v) = ver.as_ref() {
                return Ok(v.clone());
            }
        }

        let url = format!("{}/api/versions.json", DDRAGON_BASE);
        let versions: Vec<String> = self.client.get(&url).send().await?.json().await?;
        let version = versions
            .into_iter()
            .next()
            .ok_or_else(|| AppError::Custom("No versions found".into()))?;

        *self.version.write().await = Some(version.clone());
        Ok(version)
    }

    pub async fn get_champions(&self) -> Result<HashMap<String, String>, AppError> {
        {
            let champs = self.champions.read().await;
            if let Some(c) = champs.as_ref() {
                return Ok(c.clone());
            }
        }

        let _lock = self.champion_lock.lock().await;

        {
            let champs = self.champions.read().await;
            if let Some(c) = champs.as_ref() {
                return Ok(c.clone());
            }
        }

        let version = self.get_latest_version().await?;
        let url = format!(
            "{}/cdn/{}/data/ru_RU/champion.json",
            DDRAGON_BASE, version
        );
        let resp: serde_json::Value = self.client.get(&url).send().await?.json().await?;

        let mut result = HashMap::new();
        if let Some(data) = resp.get("data").and_then(|d| d.as_object()) {
            for (english_name, info) in data {
                let display_name = info
                    .get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or(english_name)
                    .to_string();
                result.insert(display_name, english_name.clone());
            }
        }

        // Load champion details in parallel (limited concurrency)
        let semaphore = Arc::new(tokio::sync::Semaphore::new(10));
        let mut handles = Vec::new();

        for (display_name, english_name) in &result {
            let sem = semaphore.clone();
            let client = self.client.clone();
            let ver = version.clone();
            let dn = display_name.clone();
            let en = english_name.clone();

            handles.push(tokio::spawn(async move {
                let _permit = sem.acquire().await.unwrap();
                let url = format!(
                    "{}/cdn/{}/data/en_US/champion/{}.json",
                    DDRAGON_BASE, ver, en
                );
                let resp: Result<serde_json::Value, _> =
                    client.get(&url).send().await?.json().await;
                match resp {
                    Ok(val) => Ok((dn, en, val)),
                    Err(e) => Err(AppError::Http(e)),
                }
            }));
        }

        let mut cache = self.champion_info_cache.write().await;
        for handle in handles {
            if let Ok(Ok((display_name, english_name, val))) = handle.await {
                if let Some(champ_data) = val
                    .get("data")
                    .and_then(|d| d.get(&english_name))
                {
                    let id = champ_data
                        .get("key")
                        .and_then(|k| k.as_str())
                        .unwrap_or("0")
                        .to_string();
                    let image_file = champ_data
                        .get("image")
                        .and_then(|i| i.get("full"))
                        .and_then(|f| f.as_str())
                        .unwrap_or("")
                        .to_string();
                    let tags: Vec<String> = champ_data
                        .get("tags")
                        .and_then(|t| t.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_str().map(String::from))
                                .collect()
                        })
                        .unwrap_or_default();

                    let mut skins = Vec::new();
                    if let Some(skin_arr) = champ_data.get("skins").and_then(|s| s.as_array()) {
                        let champ_id: i32 = id.parse().unwrap_or(0);
                        for skin in skin_arr {
                            let skin_num = skin.get("num").and_then(|n| n.as_i64()).unwrap_or(0) as i32;
                            let skin_id = skin.get("id").and_then(|n| n.as_str())
                                .and_then(|s| s.parse::<i32>().ok())
                                .unwrap_or(0);
                            let skin_name = skin
                                .get("name")
                                .and_then(|n| n.as_str())
                                .unwrap_or("default")
                                .to_string();

                            skins.push(SkinInfo {
                                id: skin_id,
                                name: skin_name.clone(),
                                skin_number: skin_num,
                                champion_name: display_name.clone(),
                                champion_id: champ_id,
                                background_skin_id: skin_id,
                                splash_url: format!(
                                    "https://ddragon.leagueoflegends.com/cdn/img/champion/splash/{}_{}.jpg",
                                    english_name, skin_num
                                ),
                            });
                        }
                    }

                    cache.insert(
                        display_name.clone(),
                        ChampionInfo {
                            display_name: display_name.clone(),
                            english_name: english_name.clone(),
                            id,
                            image_file_name: image_file,
                            tags,
                            aliases: vec![display_name.clone(), english_name.clone()],
                            skins,
                        },
                    );
                }
            }
        }

        *self.champions.write().await = Some(result.clone());
        Ok(result)
    }

    pub async fn get_champion_info(&self, display_name: &str) -> Option<ChampionInfo> {
        let cache = self.champion_info_cache.read().await;
        cache.get(display_name).cloned()
    }

    pub async fn get_champion_image_file_name(&self, display_name: &str) -> String {
        let cache = self.champion_info_cache.read().await;
        cache
            .get(display_name)
            .map(|c| c.english_name.clone())
            .unwrap_or_default()
    }

    pub fn get_champion_splashart_url(english_name: &str, skin_num: i32) -> String {
        format!(
            "https://ddragon.leagueoflegends.com/cdn/img/champion/splash/{}_{}.jpg",
            english_name, skin_num
        )
    }

    pub async fn get_champion_lanes(&self, display_name: &str) -> Vec<String> {
        let cache = self.champion_info_cache.read().await;
        if let Some(info) = cache.get(display_name) {
            let mut lanes = Vec::new();
            for tag in &info.tags {
                match tag.as_str() {
                    "Fighter" | "Tank" => {
                        if !lanes.contains(&"TOP".to_string()) {
                            lanes.push("TOP".to_string());
                        }
                    }
                    "Assassin" | "Fighter" => {
                        if !lanes.contains(&"JUNGLE".to_string()) {
                            lanes.push("JUNGLE".to_string());
                        }
                    }
                    "Mage" => lanes.push("MIDDLE".to_string()),
                    "Marksman" => lanes.push("BOTTOM".to_string()),
                    "Support" => lanes.push("UTILITY".to_string()),
                    _ => {}
                }
            }
            lanes
        } else {
            Vec::new()
        }
    }

    pub async fn get_summoner_spells(&self) -> Result<HashMap<String, String>, AppError> {
        {
            let spells = self.spells.read().await;
            if let Some(s) = spells.as_ref() {
                return Ok(s.clone());
            }
        }

        let _lock = self.spell_lock.lock().await;

        {
            let spells = self.spells.read().await;
            if let Some(s) = spells.as_ref() {
                return Ok(s.clone());
            }
        }

        let version = self.get_latest_version().await?;
        let url = format!(
            "{}/cdn/{}/data/ru_RU/summoner.json",
            DDRAGON_BASE, version
        );
        let resp: serde_json::Value = self.client.get(&url).send().await?.json().await?;

        let mut result = HashMap::new();
        if let Some(data) = resp.get("data").and_then(|d| d.as_object()) {
            for (_key, info) in data {
                // Only include spells available in Classic (Summoner's Rift) mode
                let is_classic = info
                    .get("modes")
                    .and_then(|m| m.as_array())
                    .map_or(false, |arr| {
                        arr.iter().any(|v| v.as_str() == Some("CLASSIC"))
                    });
                if !is_classic {
                    continue;
                }

                let name = info
                    .get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("")
                    .to_string();
                let spell_key = info
                    .get("key")
                    .and_then(|k| k.as_str())
                    .unwrap_or("")
                    .to_string();
                result.insert(name, spell_key);
            }
        }

        *self.spells.write().await = Some(result.clone());
        Ok(result)
    }

    pub async fn get_champion_image_url(&self, champion_name: &str) -> String {
        let version = self.get_latest_version().await.unwrap_or_default();
        let english = self.get_champion_image_file_name(champion_name).await;
        let name = if english.is_empty() {
            champion_name
        } else {
            &english
        };
        format!(
            "{}/cdn/{}/img/champion/{}.png",
            DDRAGON_BASE, version, name
        )
    }

    pub async fn get_summoner_spell_image_url(&self, spell_name: &str) -> String {
        let version = self.get_latest_version().await.unwrap_or_default();
        format!(
            "{}/cdn/{}/img/spell/{}.png",
            DDRAGON_BASE, version, spell_name
        )
    }

    pub fn get_rank_icon_url(tier: &str) -> String {
        let tier_lower = tier.to_lowercase();
        format!(
            "https://raw.communitydragon.org/latest/plugins/rcp-fe-lol-static-assets/global/default/images/ranked-mini-crests/{}.png",
            tier_lower
        )
    }

    pub fn get_profile_icon_url(version: &str, profile_icon_id: i32) -> String {
        format!(
            "{}/cdn/{}/img/profileicon/{}.png",
            DDRAGON_BASE, version, profile_icon_id
        )
    }
}
