use serde::Serialize;
use serde_json::Value;
use crate::{error::AppError, models::account::is_valid_puuid, services::riot_client::RiotClientService};

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CurrentRiotAccount {
    pub puuid: String,
    pub riot_id: String,
}
fn field<'a>(value: &'a Value, name: &str) -> Option<&'a str> {
    value.get(name)?.as_str().map(str::trim).filter(|s| !s.is_empty())
}
fn identity(summoner: &Value, chat: &Value) -> CurrentRiotAccount {
    let puuid = [field(summoner, "puuid"), field(chat, "puuid")].into_iter().flatten()
        .find(|p| is_valid_puuid(p)).or_else(|| field(summoner, "puuid")).unwrap_or_default();
    let name = field(summoner, "gameName").or_else(|| field(chat, "gameName"));
    let tag = field(summoner, "tagLine").or_else(|| field(chat, "gameTag")).or_else(|| field(chat, "tagLine"));
    let riot_id = match (name, tag) {
        (Some(name), Some(tag)) => format!("{name}#{tag}"),
        _ => field(summoner, "displayName").filter(|s| s.contains('#')).unwrap_or_default().into(),
    };
    CurrentRiotAccount { puuid: puuid.into(), riot_id }
}

pub async fn current_account(client: &RiotClientService) -> Result<CurrentRiotAccount, AppError> {
    // Friend discovery needs identity only, not the ranked/statistics requests in get_account_info.
    let summoner: Value = serde_json::from_str(&client.lcu_get("/lol-summoner/v1/current-summoner").await?)?;
    let preliminary = identity(&summoner, &Value::Null);
    let chat = if !is_valid_puuid(&preliminary.puuid) || preliminary.riot_id.is_empty() {
        match client.lcu_get("/lol-chat/v1/me").await {
            Ok(raw) => serde_json::from_str(&raw).unwrap_or(Value::Null),
            Err(_) => Value::Null,
        }
    } else { Value::Null };
    if let (Some(before), Some(after)) = (field(&summoner, "gameName"), field(&chat, "gameName")) {
        if before.to_lowercase() != after.to_lowercase() {
            return Err(AppError::Custom("Riot-аккаунт изменился во время чтения друзей. Повторите обновление".into()));
        }
    }
    Ok(identity(&summoner, &chat))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn chat_fills_partial_summoner_identity_without_accepting_a_guid_as_puuid() {
        let puuid = "a".repeat(78);
        let current = identity(&json!({"puuid":"00000000-0000-0000-0000-000000000000","gameName":"Игрок"}),
            &json!({"puuid":puuid,"gameName":"Игрок","gameTag":"RU1"}));
        assert_eq!(current.puuid, puuid);
        assert_eq!(current.riot_id, "Игрок#RU1");
        let complete = identity(&json!({"puuid":"b".repeat(78),"gameName":"Current","tagLine":"EUW"}), &json!({"puuid":puuid}));
        assert_eq!(complete.puuid, "b".repeat(78));
        assert!(identity(&Value::Null, &Value::Null).riot_id.is_empty());
    }
    #[tokio::test]
    #[ignore = "requires an explicit RUSTLM_TEST_LCU simulator config"]
    async fn simulator_friend_identity_uses_live_account_and_chat_fallback() {
        assert!(std::env::var_os("RUSTLM_TEST_LCU").is_some());
        let client = RiotClientService::new();
        let mut fixture: Value = serde_json::from_str(&client.lcu_get("/__test/state").await.unwrap()).unwrap();
        fixture["summoner"] = json!({"puuid":"partial-guid","gameName":"First"});
        fixture["chatMe"] = json!({"puuid":"a".repeat(78),"gameName":"First","gameTag":"RU1"});
        client.lcu_post("/__test/fixture", &fixture.to_string()).await.unwrap();
        assert_eq!(current_account(&client).await.unwrap().puuid, "a".repeat(78));
        fixture["chatMe"]["gameName"] = json!("Different");
        client.lcu_post("/__test/fixture", &fixture.to_string()).await.unwrap();
        assert!(current_account(&client).await.is_err());
        fixture["summoner"] = json!({"puuid":"b".repeat(78),"gameName":"Second","tagLine":"EUW"});
        client.lcu_post("/__test/fixture", &fixture.to_string()).await.unwrap();
        let current = current_account(&client).await.unwrap();
        assert_eq!(current.puuid, "b".repeat(78));
        assert_eq!(current.riot_id, "Second#EUW");
    }
}
