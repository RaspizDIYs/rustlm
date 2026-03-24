#![allow(dead_code)]

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rune {
    pub id: i32,
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub short_desc: String,
    #[serde(default)]
    pub long_desc: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuneSlot {
    #[serde(default)]
    pub runes: Vec<Rune>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunePath {
    pub id: i32,
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub slots: Vec<RuneSlot>,
}

impl RunePath {
    pub fn color_hex(&self) -> &str {
        match self.key.as_str() {
            "Precision" => "#C8AA6E",
            "Domination" => "#C83C51",
            "Sorcery" => "#6C8CD5",
            "Resolve" => "#A1D586",
            "Inspiration" => "#48C9B0",
            _ => "#6C8CD5",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct RunePage {
    #[serde(default = "default_rune_page_name")]
    pub name: String,
    #[serde(default)]
    pub primary_path_id: i32,
    #[serde(default)]
    pub secondary_path_id: i32,
    #[serde(default)]
    pub primary_keystone_id: i32,
    #[serde(default)]
    pub primary_slot1_id: i32,
    #[serde(default)]
    pub primary_slot2_id: i32,
    #[serde(default)]
    pub primary_slot3_id: i32,
    #[serde(default)]
    pub secondary_slot1_id: i32,
    #[serde(default)]
    pub secondary_slot2_id: i32,
    #[serde(default)]
    pub secondary_slot3_id: i32,
    #[serde(default)]
    pub stat_mod1_id: i32,
    #[serde(default)]
    pub stat_mod2_id: i32,
    #[serde(default)]
    pub stat_mod3_id: i32,
}

fn default_rune_page_name() -> String {
    "Новая страница рун".to_string()
}

impl Default for RunePage {
    fn default() -> Self {
        Self {
            name: default_rune_page_name(),
            primary_path_id: 0,
            secondary_path_id: 0,
            primary_keystone_id: 0,
            primary_slot1_id: 0,
            primary_slot2_id: 0,
            primary_slot3_id: 0,
            secondary_slot1_id: 0,
            secondary_slot2_id: 0,
            secondary_slot3_id: 0,
            stat_mod1_id: 0,
            stat_mod2_id: 0,
            stat_mod3_id: 0,
        }
    }
}
