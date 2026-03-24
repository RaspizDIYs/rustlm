use std::sync::Arc;

use crate::error::AppError;
use crate::services::riot_client::RiotClientService;

pub struct CustomizationService {
    riot_client: Arc<RiotClientService>,
}

impl CustomizationService {
    pub fn new(riot_client: Arc<RiotClientService>) -> Self {
        Self { riot_client }
    }

    pub async fn set_profile_status(&self, status: &str) -> Result<bool, AppError> {
        let body = serde_json::json!({ "statusMessage": status }).to_string();
        let resp = self.riot_client.lcu_put("/lol-chat/v1/me", &body).await?;
        Ok(!resp.contains("errorCode"))
    }

    pub async fn set_profile_availability(&self, availability: &str) -> Result<bool, AppError> {
        let body = serde_json::json!({ "availability": availability }).to_string();
        let resp = self.riot_client.lcu_put("/lol-chat/v1/me", &body).await?;
        Ok(!resp.contains("errorCode"))
    }

    pub async fn set_profile_icon(&self, icon_id: i32) -> Result<bool, AppError> {
        let body = serde_json::json!({ "profileIconId": icon_id }).to_string();
        let resp = self.riot_client.lcu_put("/lol-chat/v1/me", &body).await?;
        Ok(!resp.contains("errorCode"))
    }

    pub async fn set_profile_background(&self, background_skin_id: i32) -> Result<bool, AppError> {
        // Try multiple approaches since this endpoint varies
        let body = serde_json::json!({ "backgroundSkinId": background_skin_id }).to_string();

        // Try POST first
        if let Ok(resp) = self.riot_client.lcu_post(
            "/lol-summoner/v1/current-summoner/summoner-profile",
            &body,
        ).await {
            if !resp.contains("errorCode") {
                return Ok(true);
            }
        }

        // Try PUT
        if let Ok(resp) = self.riot_client.lcu_put(
            "/lol-summoner/v1/current-summoner/summoner-profile",
            &body,
        ).await {
            if !resp.contains("errorCode") {
                return Ok(true);
            }
        }

        // Try PATCH
        let resp = self.riot_client.lcu_patch(
            "/lol-summoner/v1/current-summoner/summoner-profile",
            &body,
        ).await?;

        Ok(!resp.contains("errorCode"))
    }

    pub async fn get_challenges(&self) -> Result<serde_json::Value, AppError> {
        let resp = self.riot_client.lcu_get("/lol-challenges/v1/challenges/local-player").await?;
        serde_json::from_str(&resp).map_err(|e| AppError::Json(e))
    }

    pub async fn set_challenge_tokens(
        &self,
        challenge_ids: &[i64],
        title_id: i64,
    ) -> Result<bool, AppError> {
        let body = serde_json::json!({
            "challengeIds": challenge_ids,
            "title": title_id
        }).to_string();

        let resp = self.riot_client.lcu_post(
            "/lol-challenges/v1/update-player-preferences",
            &body,
        ).await?;

        Ok(!resp.contains("errorCode"))
    }
}
