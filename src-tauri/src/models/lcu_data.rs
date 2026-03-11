use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LcuRunePage {
    pub id: i32,
    #[serde(default)]
    pub name: String,
    pub primary_style_id: i32,
    pub sub_style_id: i32,
    #[serde(default)]
    pub selected_perk_ids: Vec<i32>,
    #[serde(default)]
    pub current: bool,
    #[serde(default)]
    pub is_editable: bool,
    #[serde(default)]
    pub is_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LcuPerk {
    pub id: i32,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub icon_path: String,
}
