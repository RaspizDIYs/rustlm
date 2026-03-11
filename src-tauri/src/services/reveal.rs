use std::sync::Arc;
use std::time::Duration;

use tokio::sync::RwLock;

use crate::error::AppError;
use crate::models::player::PlayerInfo;
use crate::services::riot_client::RiotClientService;

pub struct RevealService {
    riot_client: Arc<RiotClientService>,
    api_key: RwLock<Option<String>>,
    region: RwLock<String>,
    http_client: reqwest::Client,
}

impl RevealService {
    pub fn new(riot_client: Arc<RiotClientService>) -> Self {
        Self {
            riot_client,
            api_key: RwLock::new(None),
            region: RwLock::new("euw1".to_string()),
            http_client: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .unwrap(),
        }
    }

    pub async fn set_api_key(&self, api_key: &str) {
        *self.api_key.write().await = Some(api_key.to_string());
    }

    pub async fn set_region(&self, region: &str) {
        *self.region.write().await = region.to_string();
    }

    pub async fn set_api_configuration(&self, api_key: &str, region: &str) {
        self.set_api_key(api_key).await;
        self.set_region(region).await;
    }

    fn regional_host(region: &str) -> &'static str {
        match region {
            "euw1" | "eun1" | "tr1" | "ru" => "europe",
            "na1" | "br1" | "la1" | "la2" | "oc1" => "americas",
            "kr" | "jp1" => "asia",
            "ph2" | "sg2" | "th2" | "tw2" | "vn2" => "sea",
            _ => "europe",
        }
    }

    async fn riot_api_get(&self, url: &str) -> Result<String, AppError> {
        let api_key = self.api_key.read().await;
        let key = api_key
            .as_deref()
            .ok_or_else(|| AppError::Custom("API key not configured".into()))?;

        let resp = self
            .http_client
            .get(url)
            .header("X-Riot-Token", key)
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(AppError::Custom(format!(
                "Riot API error: {}",
                resp.status()
            )));
        }

        resp.text().await.map_err(|e| AppError::Http(e))
    }

    pub async fn test_api_key(&self) -> Result<(bool, String), AppError> {
        let region = self.region.read().await.clone();
        let url = format!(
            "https://{}.api.riotgames.com/lol/status/v4/platform-data",
            region
        );

        match self.riot_api_get(&url).await {
            Ok(_) => Ok((true, "API key valid".to_string())),
            Err(e) => Ok((false, e.to_string())),
        }
    }

    pub async fn get_account_by_riot_id(
        &self,
        game_name: &str,
        tag_line: &str,
    ) -> Result<String, AppError> {
        let region = self.region.read().await.clone();
        let host = Self::regional_host(&region);
        let url = format!(
            "https://{}.api.riotgames.com/riot/account/v1/accounts/by-riot-id/{}/{}",
            host, game_name, tag_line
        );
        self.riot_api_get(&url).await
    }

    pub async fn get_summoner_by_puuid(
        &self,
        puuid: &str,
    ) -> Result<String, AppError> {
        let region = self.region.read().await.clone();
        let url = format!(
            "https://{}.api.riotgames.com/lol/summoner/v4/summoners/by-puuid/{}",
            region, puuid
        );
        self.riot_api_get(&url).await
    }

    pub async fn get_ranked_stats_by_puuid(
        &self,
        puuid: &str,
    ) -> Result<String, AppError> {
        let region = self.region.read().await.clone();
        let url = format!(
            "https://{}.api.riotgames.com/lol/league/v4/entries/by-puuid/{}",
            region, puuid
        );
        self.riot_api_get(&url).await
    }

    pub async fn get_teams_info(
        &self,
    ) -> Result<(Vec<PlayerInfo>, Vec<PlayerInfo>), AppError> {
        let session_resp = self
            .riot_client
            .lcu_get("/lol-champ-select/v1/session")
            .await?;

        let session: serde_json::Value = serde_json::from_str(&session_resp)?;

        let mut allies = Vec::new();
        let mut enemies = Vec::new();

        // Parse myTeam
        if let Some(my_team) = session.get("myTeam").and_then(|t| t.as_array()) {
            for member in my_team {
                if let Some(info) = self.parse_team_member(member).await {
                    allies.push(info);
                }
            }
        }

        // Parse theirTeam
        if let Some(their_team) = session.get("theirTeam").and_then(|t| t.as_array()) {
            for member in their_team {
                if let Some(info) = self.parse_team_member(member).await {
                    enemies.push(info);
                }
            }
        }

        Ok((allies, enemies))
    }

    async fn parse_team_member(&self, member: &serde_json::Value) -> Option<PlayerInfo> {
        let summoner_id = member.get("summonerId").and_then(|s| s.as_i64())?;
        let champion_id = member.get("championId").and_then(|c| c.as_i64()).unwrap_or(0);

        // Try to get summoner info from LCU
        let summoner_resp = self
            .riot_client
            .lcu_get(&format!("/lol-summoner/v1/summoners/{}", summoner_id))
            .await
            .ok()?;

        let summoner: serde_json::Value = serde_json::from_str(&summoner_resp).ok()?;

        let game_name = summoner
            .get("gameName")
            .and_then(|g| g.as_str())
            .unwrap_or("")
            .to_string();
        let tag_line = summoner
            .get("tagLine")
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string();
        let puuid = summoner
            .get("puuid")
            .and_then(|p| p.as_str())
            .unwrap_or("")
            .to_string();
        let profile_icon = summoner
            .get("profileIconId")
            .and_then(|p| p.as_i64())
            .unwrap_or(0) as i32;
        let level = summoner
            .get("summonerLevel")
            .and_then(|l| l.as_i64())
            .unwrap_or(0) as i32;

        // Try to get ranked info
        let mut tier = "Unranked".to_string();
        let mut rank = String::new();
        let mut lp = 0;
        let mut wins = 0;
        let mut losses = 0;

        if let Ok(ranked_resp) = self.get_ranked_stats_by_puuid(&puuid).await {
            if let Ok(entries) = serde_json::from_str::<Vec<serde_json::Value>>(&ranked_resp) {
                for entry in entries {
                    let queue = entry.get("queueType").and_then(|q| q.as_str()).unwrap_or("");
                    if queue == "RANKED_SOLO_5x5" {
                        tier = entry.get("tier").and_then(|t| t.as_str()).unwrap_or("Unranked").to_string();
                        rank = entry.get("rank").and_then(|r| r.as_str()).unwrap_or("").to_string();
                        lp = entry.get("leaguePoints").and_then(|l| l.as_i64()).unwrap_or(0) as i32;
                        wins = entry.get("wins").and_then(|w| w.as_i64()).unwrap_or(0) as i32;
                        losses = entry.get("losses").and_then(|l| l.as_i64()).unwrap_or(0) as i32;
                        break;
                    }
                }
            }
        }

        let riot_id = if tag_line.is_empty() {
            game_name.clone()
        } else {
            format!("{}#{}", game_name, tag_line)
        };

        let win_rate = if wins + losses > 0 {
            format!("{:.0}%", (wins as f64 / (wins + losses) as f64) * 100.0)
        } else {
            "N/A".to_string()
        };

        Some(PlayerInfo {
            riot_id,
            summoner_name: game_name,
            champion_id: champion_id as i32,
            tier,
            rank,
            league_points: lp,
            wins,
            losses,
            win_rate,
            level,
            profile_icon_id: profile_icon,
            puuid,
            ugg_link: String::new(),
        })
    }

    pub async fn send_message_to_chat(&self, message: &str) -> Result<bool, AppError> {
        let body = serde_json::json!({
            "body": message,
            "type": "chat"
        }).to_string();

        let resp = self
            .riot_client
            .lcu_post("/lol-chat/v1/conversations/champ-select/messages", &body)
            .await?;

        Ok(!resp.contains("errorCode"))
    }

    pub fn generate_ugg_link(summoner_name: &str, region: &str) -> String {
        let name = summoner_name.replace('#', "-");
        format!("https://u.gg/lol/profile/{}/{}/overview", region, name)
    }
}
