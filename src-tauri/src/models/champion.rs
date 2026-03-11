use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChampionInfo {
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub english_name: String,
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub image_file_name: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub skins: Vec<SkinInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkinInfo {
    pub id: i32,
    #[serde(default)]
    pub name: String,
    pub skin_number: i32,
    #[serde(default)]
    pub champion_name: String,
    pub champion_id: i32,
    pub background_skin_id: i32,
    #[serde(default)]
    pub splash_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChallengeInfo {
    pub id: i64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub icon_url: String,
    #[serde(default)]
    pub category: String,
}
