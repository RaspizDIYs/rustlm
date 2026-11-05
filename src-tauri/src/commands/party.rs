use super::league_sharing as league;
use crate::{error::AppError, state::AppState};
use serde_json::{json, Value};
use std::{
    collections::BTreeSet,
    time::{Duration, Instant},
};
use tauri::{Emitter, Manager, State};

static ACTION: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn error(message: &str) -> AppError {
    AppError::Custom(message.into())
}
fn valid_party_id(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|c| c.is_ascii_hexdigit())
}
fn id(value: &Value) -> Option<String> {
    match value {
        Value::String(s)
            if !s.is_empty() && s.len() <= 20 && s.bytes().all(|c| c.is_ascii_digit()) =>
        {
            s.parse::<u64>()
                .ok()
                .filter(|v| *v > 0)
                .map(|v| v.to_string())
        }
        Value::Number(n) => n.as_u64().filter(|v| *v > 0).map(|v| v.to_string()),
        _ => None,
    }
}
fn riot_id(value: &Value) -> Option<String> {
    let name = value["gameName"].as_str()?;
    let tag = value["tagLine"]
        .as_str()
        .or_else(|| value["gameTag"].as_str())?;
    (!name.trim().is_empty() && !tag.trim().is_empty())
        .then(|| format!("{}#{}", name.trim(), tag.trim()))
}
async fn optional_lcu(state: &AppState, path: &str) -> Result<Value, AppError> {
    match state.riot_client.lcu_get(path).await {
        Ok(raw) => Ok(serde_json::from_str(&raw)?),
        Err(e) if e.to_string().contains("returned 404") => Ok(Value::Null),
        Err(e) => Err(e),
    }
}
fn local_empty(message: Option<&str>) -> Value {
    json!({"phase":"Unknown","profileIconId":0,"automation":null,"riotId":"","puuid":"","summonerId":"","lobbyId":"","lobbyQueueId":null,"lobbyLeader":false,"lobbyMemberIds":[],"matchingInvitation":null,"error":message})
}
fn rank_display(stats: &Value) -> Option<String> {
    let queues = stats["queues"].as_array()?;
    for kind in ["RANKED_SOLO_5x5", "RANKED_FLEX_SR"] {
        let Some(queue) = queues.iter().find(|q| q["queueType"] == kind) else {
            continue;
        };
        let Some(tier) = queue["tier"].as_str() else {
            continue;
        };
        if ![
            "IRON",
            "BRONZE",
            "SILVER",
            "GOLD",
            "PLATINUM",
            "EMERALD",
            "DIAMOND",
            "MASTER",
            "GRANDMASTER",
            "CHALLENGER",
        ]
        .contains(&tier)
        {
            continue;
        }
        let Some(division) = queue["division"].as_str() else {
            continue;
        };
        if !["I", "II", "III", "IV"].contains(&division) {
            continue;
        }
        let Some(lp) = queue["leaguePoints"]
            .as_i64()
            .filter(|lp| *lp >= 0 && *lp <= i32::MAX as i64)
        else {
            continue;
        };
        return Some(
            crate::models::player::PlayerInfo {
                tier: tier.into(),
                rank: division.into(),
                league_points: lp as i32,
                ..Default::default()
            }
            .full_rank(),
        );
    }
    None
}
async fn current_rank(state: &AppState, summoner: &Value) -> Option<String> {
    // Keep optional rank reads from delaying party actions when LCU is unavailable.
    tokio::time::timeout(Duration::from_millis(750), async {
        let raw = state
            .riot_client
            .lcu_get("/lol-ranked/v1/current-ranked-stats")
            .await
            .ok()?;
        let rank = rank_display(&serde_json::from_str::<Value>(&raw).ok()?)?;
        let raw = state
            .riot_client
            .lcu_get("/lol-summoner/v1/current-summoner")
            .await
            .ok()?;
        let fresh: Value = serde_json::from_str(&raw).ok()?;
        (id(&fresh["summonerId"]) == id(&summoner["summonerId"])
            && fresh["puuid"].as_str().filter(|p| !p.is_empty())
                == summoner["puuid"].as_str().filter(|p| !p.is_empty())
            && riot_id(&fresh) == riot_id(summoner))
        .then_some(rank)
    })
    .await
    .ok()
    .flatten()
}
fn game_outcome(session: &Value, own: &Value) -> (String, u64) {
    let Some(game) = id(&session["gameData"]["gameId"]) else {
        return (String::new(), 0);
    };
    let players: Vec<_> = ["teamOne", "teamTwo"]
        .into_iter()
        .flat_map(|team| session["gameData"][team].as_array().into_iter().flatten())
        .filter(|player| {
            let puuid = own["puuid"].as_str().filter(|p| !p.is_empty());
            if let Some(puuid) = puuid {
                player["puuid"].as_str() == Some(puuid)
            } else {
                id(&own["summonerId"]).is_some()
                    && id(&player["summonerId"]) == id(&own["summonerId"])
            }
        })
        .collect();
    if players.len() != 1 {
        return (String::new(), 0);
    }
    let Some(champion) = players[0]["championId"]
        .as_u64()
        .filter(|c| (1..=10000).contains(c))
    else {
        return (String::new(), 0);
    };
    (game, champion)
}
async fn read_local(state: &AppState) -> Result<(Value, Value), AppError> {
    let started = Instant::now();
    let phase: Value = serde_json::from_str(
        &state
            .riot_client
            .lcu_get("/lol-gameflow/v1/gameflow-phase")
            .await?,
    )?;
    let me = league::identity(state).await?;
    let summoner: Value = serde_json::from_str(
        &state
            .riot_client
            .lcu_get("/lol-summoner/v1/current-summoner")
            .await?,
    )?;
    let summoner_id =
        id(&summoner["summonerId"]).ok_or_else(|| error("League ещё не загрузила профиль"))?;
    if riot_id(&summoner).is_some_and(|name| name.to_lowercase() != me.riot_id.to_lowercase()) {
        return Err(error("Аккаунт League изменился"));
    }
    let lobby = optional_lcu(state, "/lol-lobby/v2/lobby").await?;
    if started.elapsed() > Duration::from_secs(3) {
        return Err(error(
            "League отвечает медленно; готовность не подтверждена",
        ));
    }
    let members: Vec<String> = lobby["members"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| id(&m["summonerId"]))
        .collect();
    let leader = lobby["members"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|m| m["isLeader"] == true)
        .and_then(|m| id(&m["summonerId"]));
    let queue = lobby["gameConfig"]["queueId"].as_i64();
    let lobby_id = lobby["partyId"]
        .as_str()
        .map(str::to_owned)
        .or_else(|| {
            leader
                .as_ref()
                .zip(queue)
                .map(|(leader, queue)| format!("{queue}:{leader}"))
        })
        .unwrap_or_default();
    let role = lobby["localMember"]["firstPositionPreference"]
        .as_str()
        .unwrap_or("")
        .to_lowercase();
    let automation = if phase == "ChampSelect" {
        crate::commands::draft_coordination::local(state).await.ok().map(|v|json!({"mode":v.intent.mode,"autoPick":v.auto_pick_enabled && state.settings.load_setting(crate::services::draft_coordination::OPT_IN,true),"autoBan":v.auto_ban_enabled && state.settings.load_setting(crate::services::draft_coordination::OPT_IN,true),"preferences":v.intent.preferences}))
    } else {
        crate::commands::draft_coordination::automation_summary(state, queue.unwrap_or(0), &role)
            .await
    };
    let server = state.riot_client.detect_server().await.unwrap_or_default();
    let rank = if summoner["puuid"].as_str() == Some(me.puuid.as_str()) && !me.puuid.is_empty() {
        current_rank(state, &summoner).await
    } else {
        None
    };
    let outcome = if phase == "InProgress" {
        optional_lcu(state, "/lol-gameflow/v1/session")
            .await
            .ok()
            .map(|session| game_outcome(&session, &summoner))
            .unwrap_or_else(|| (String::new(), 0))
    } else {
        (String::new(), 0)
    };
    let is_leader = lobby["localMember"]["isLeader"].as_bool() == Some(true)
        && id(&lobby["localMember"]["summonerId"]).as_ref() == Some(&summoner_id);
    Ok((
        json!({"phase":phase.as_str().unwrap_or("Unknown"),"profileIconId":summoner["profileIconId"].as_u64().filter(|id| *id <= 1000000).unwrap_or(0),"riotId":me.riot_id,"puuid":me.puuid,"summonerId":summoner_id,"lobbyId":lobby_id,"lobbyQueueId":queue,"lobbyLeader":is_leader,"lobbyMemberIds":members,"matchingInvitation":null,"error":null,"automation":automation,"server":server,"rankDisplay":rank,"gameId":outcome.0,"actualChampionId":outcome.1,"lobbyCount":members.len(),"lobbyReady":is_leader && queue.is_some()}),
        lobby,
    ))
}
fn party_limit(queue: i64) -> Option<usize> {
    match queue {
        420 => Some(2),
        1700 | 1710 | 1750 => Some(3),
        400 | 430 | 440 | 480 | 490 | 450 | 2400 => Some(5),
        _ => None,
    }
}
fn party_size_allowed(queue: i64, size: usize) -> bool {
    party_limit(queue).is_some_and(|max| size > 0 && size <= max && !(queue == 440 && size == 4))
}
fn queue_capacity(queue: &Value) -> Option<usize> {
    let id = queue["id"].as_i64()?;
    let mut capacity = party_limit(id)?;
    for field in ["maximumParticipantListSize", "maxLobbySize"] {
        if let Some(limit) = queue[field].as_u64().filter(|v| *v > 0 && *v <= 100) {
            capacity = capacity.min(limit as usize);
        }
    }
    if matches!(id, 1700 | 1710 | 1750) {
        if let Some(team) = queue["numPlayersPerTeam"]
            .as_u64()
            .filter(|v| *v > 0 && *v <= 16)
        {
            capacity = capacity.min(team as usize);
        }
    }
    Some(capacity)
}
async fn queues(state: &AppState) -> Vec<Value> {
    static CACHE: tokio::sync::Mutex<Option<(Instant, Vec<Value>)>> =
        tokio::sync::Mutex::const_new(None);
    let mut cache = CACHE.lock().await;
    if let Some((at, values)) = cache.as_ref() {
        if at.elapsed() < Duration::from_secs(300) {
            return values.clone();
        }
    }
    let Ok(raw) = state
        .riot_client
        .lcu_get("/lol-game-queues/v1/queues")
        .await
    else {
        return cache.as_ref().map(|v| v.1.clone()).unwrap_or_default();
    };
    let Ok(value) = serde_json::from_str::<Value>(&raw) else {
        return vec![];
    };
    let list:Vec<Value>=value.as_array().into_iter().flatten().filter(|q|q["id"].as_i64().is_some_and(|id|party_limit(id).is_some()) && q["queueAvailability"].as_str()==Some("Available") && q["isCustom"]!=true)
        .filter_map(|q|Some(json!({"id":q["id"].as_i64()?,"name":q["description"].as_str().or_else(||q["name"].as_str())?,"maxPlayers":queue_capacity(q)}))).collect();
    *cache = Some((Instant::now(), list.clone()));
    list
}
fn active<'a>(snapshot: &'a Value, party_id: &str) -> Result<&'a Value, AppError> {
    let party = &snapshot["active"];
    if !valid_party_id(party_id) || party["id"].as_str() != Some(party_id) {
        return Err(error("Группа недоступна. Обновите список"));
    }
    Ok(party)
}
fn accepted(party: &Value) -> Result<Vec<&Value>, AppError> {
    let members = party["members"]
        .as_array()
        .ok_or_else(|| error("Некорректный состав группы"))?;
    if members.is_empty() || members.len() > 5 {
        return Err(error("Допустимо от одного до пяти участников"));
    }
    Ok(members.iter().filter(|m| m["accepted"] == true).collect())
}
fn is_leader(snapshot: &Value, party: &Value) -> bool {
    snapshot["me"]
        .as_str()
        .is_some_and(|me| party["leaderId"].as_str() == Some(me))
}
fn lobby_matches(party: &Value, lobby: &Value, complete: bool) -> Result<(), AppError> {
    let queue = party["queueId"]
        .as_i64()
        .ok_or_else(|| error("Неизвестный режим группы"))?;
    let count = accepted(party)?.len();
    if party_limit(queue).is_none_or(|max| count > max)
        || (complete && !party_size_allowed(queue, count))
    {
        return Err(error("Состав группы не подходит для выбранного режима"));
    }
    if lobby["gameConfig"]["queueId"] != party["queueId"]
        || lobby["localMember"]["isLeader"] != true
    {
        return Err(error(
            "Откройте лобби выбранного режима и получите лидерство",
        ));
    }
    let wanted: Vec<String> = accepted(party)?
        .into_iter()
        .map(|m| id(&m["gameSummonerId"]).ok_or_else(|| error("Участник ещё не загрузил League")))
        .collect::<Result<_, _>>()?;
    let expected: BTreeSet<_> = wanted.iter().cloned().collect();
    if expected.len() != wanted.len() {
        return Err(error("Не удалось различить игровые аккаунты участников"));
    }
    let actual: Vec<String> = lobby["members"]
        .as_array()
        .ok_or_else(|| error("Состав лобби неизвестен"))?
        .iter()
        .map(|m| id(&m["summonerId"]).ok_or_else(|| error("Состав лобби неизвестен")))
        .collect::<Result<_, _>>()?;
    if actual.iter().collect::<BTreeSet<_>>().len() != actual.len()
        || actual.iter().any(|m| !expected.contains(m))
        || (complete && actual.len() != expected.len())
    {
        return Err(error("Состав League отличается от группы LM"));
    }
    if lobby["gameConfig"]["maxLobbySize"]
        .as_u64()
        .is_some_and(|max| wanted.len() as u64 > max)
    {
        return Err(error("Для этого режима в группе слишком много игроков"));
    }
    if complete && lobby["canStartActivity"] != true {
        return Err(error("League пока не разрешает начать поиск"));
    }
    Ok(())
}
fn matching_invitation(party: &Value, invitations: &Value) -> Option<String> {
    let leader = party["members"]
        .as_array()?
        .iter()
        .find(|m| m["id"] == party["leaderId"] && m["accepted"] == true && m["online"] == true)?;
    let summoner = id(&leader["gameSummonerId"])?;
    invitations
        .as_array()?
        .iter()
        .find(|i| {
            i["state"] == "Pending"
                && i["canAcceptInvitation"] == true
                && id(&i["fromSummonerId"]).as_ref() == Some(&summoner)
                && i["gameConfig"]["queueId"] == party["queueId"]
        })
        .and_then(|i| i["invitationId"].as_str())
        .filter(|id| {
            !id.is_empty()
                && id.len() <= 128
                && id
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b':'))
        })
        .map(str::to_owned)
}
async fn auth_lock() -> Result<tokio::sync::MutexGuard<'static, ()>, AppError> {
    tokio::time::timeout(Duration::from_secs(3), league::LEAGUE_LOCK.lock())
        .await
        .map_err(|_| error("Дождитесь завершения подключения League"))
}
async fn remote(token: &str, local: &Value) -> Result<Value, AppError> {
    league::request(reqwest::Method::POST,"/party/status",Some(token),Some(json!({"gamePuuid":local["puuid"],"gameRiotId":local["riotId"],"gameSummonerId":local["summonerId"],"phase":local["phase"],"lobbyId":local["lobbyId"],"profileIconId":local["profileIconId"],"automation":local["automation"],"server":local["server"].as_str().unwrap_or(""),"rankDisplay":local["rankDisplay"],"gameId":local["gameId"].as_str().unwrap_or(""),"actualChampionId":local["actualChampionId"].as_u64().unwrap_or(0),"lobbyCount":local["lobbyMemberIds"].as_array().map(Vec::len).unwrap_or(0),"lobbyReady":local["lobbyLeader"]==true && local["lobbyQueueId"].is_number()}))).await
}
// Bind queued/selection phases to the last roster actually observed together in League.
fn group_together(data: &Value, local: &Value) -> Option<bool> {
    use std::sync::{Mutex, OnceLock};
    type Binding = (String, String, BTreeSet<String>);
    static BINDING: OnceLock<Mutex<Option<Binding>>> = OnceLock::new();
    let mut previous = BINDING.get_or_init(|| Mutex::new(None)).lock().ok()?;
    let phase = local["phase"].as_str()?;
    let party = &data["active"];
    let group = party["members"].as_array();
    let expected: BTreeSet<String> = group
        .into_iter()
        .flatten()
        .filter(|m| m["accepted"] == true)
        .filter_map(|m| id(&m["gameSummonerId"]))
        .collect();
    let key = party["id"].as_str().unwrap_or("").to_owned();
    let own = id(&local["summonerId"])?;
    if key.is_empty() || expected.len() < 2 {
        *previous = None;
        return Some(false);
    }
    let actual: BTreeSet<String> = local["lobbyMemberIds"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(id)
        .collect();
    if !actual.is_empty() {
        let together = expected.is_subset(&actual) && actual.contains(&own);
        *previous = if together {
            Some((key, own, expected))
        } else {
            None
        };
        return Some(together);
    }
    if matches!(phase, "None" | "Lobby") {
        *previous = None;
        return Some(false);
    }
    if matches!(
        phase,
        "Matchmaking" | "ReadyCheck" | "ChampSelect" | "GameStart" | "InProgress" | "Reconnect"
    ) {
        return previous
            .as_ref()
            .map(|v| v.0 == key && v.1 == own && v.2 == expected);
    }
    None
}
fn automation_status(data: &Value, local: &Value, together: Option<bool>) -> &'static str {
    let roster = local["lobbyMemberIds"]
        .as_array()
        .map(Vec::len)
        .unwrap_or(0);
    if together == Some(false) {
        return if roster < 2 { "solo" } else { "not-linked" };
    }
    if together.is_none() {
        return "waiting";
    }
    let Some(members) = data["active"]["members"].as_array() else {
        return "not-linked";
    };
    let mode = local["automation"]["mode"].as_str();
    for member in members.iter().filter(|m| m["accepted"] == true) {
        let summary = &member["automation"];
        if summary.is_null()
            || summary["mode"].as_str() != mode
            || !member["online"].as_bool().unwrap_or(false)
        {
            return "waiting";
        }
        if summary["autoPick"] != true && summary["autoBan"] != true {
            return "disabled";
        }
        let configured = [("pick", "autoPick"), ("ban", "autoBan")]
            .iter()
            .any(|(pool, toggle)| {
                summary[*toggle] == true
                    && summary["preferences"][*pool]
                        .as_array()
                        .is_some_and(|ps| ps.iter().any(|p| p["state"] != "exclude"))
            });
        if !configured {
            return "empty";
        }
    }
    "active"
}
fn party_server(party: &Value) -> Option<String> {
    let leader = party["members"]
        .as_array()?
        .iter()
        .find(|m| m["id"] == party["leaderId"])?;
    let server = leader["server"].as_str().filter(|s| !s.is_empty())?;
    Some(crate::services::riot_client::platform_to_server(
        server.trim(),
    ))
}
fn available_party(data: &Value, party_id: &str) -> Option<Value> {
    if data["active"]["id"] == party_id {
        return Some(data["active"].clone());
    }
    data["invitations"]
        .as_array()?
        .iter()
        .find(|p| p["id"] == party_id)
        .cloned()
}
fn switchable_party(snapshot: &Value) -> bool {
    let party = &snapshot["active"];
    matches!(party["state"].as_str(), Some("forming" | "queued"))
        && party["id"].as_str().is_some_and(valid_party_id)
        && party["revision"].as_u64().is_some()
        && accepted(party).is_ok_and(|members| members.iter().any(|m| m["id"] == snapshot["me"]))
}
fn join_target_ready(party: &Value, server: &str) -> bool {
    party["state"] == "forming"
        && party["revision"].as_u64().is_some()
        && party_server(party).as_deref() == Some(server)
        && party["members"].as_array().is_some_and(|ms| {
            ms.iter().any(|m| {
                m["id"] == party["leaderId"]
                    && m["accepted"] == true
                    && m["online"] == true
                    && m["lobbyReady"] == true
            })
        })
}
fn source_lobby_matches(snapshot: &Value, local: &Value, lobby: &Value) -> bool {
    if !switchable_party(snapshot) || !matches!(local["phase"].as_str(), Some("None" | "Lobby")) {
        return false;
    }
    if lobby.is_null() {
        return local["phase"] == "None";
    }
    let party = &snapshot["active"];
    let own = accepted(party)
        .ok()
        .and_then(|ms| ms.into_iter().find(|m| m["id"] == snapshot["me"]));
    local["phase"] == "Lobby"
        && local["lobbyId"].as_str().is_some_and(|s| !s.is_empty())
        && own.is_some_and(|m| {
            m["lobbyId"] == local["lobbyId"]
                && id(&m["gameSummonerId"])
                    .is_some_and(|sid| id(&local["summonerId"]).as_ref() == Some(&sid))
        })
        && lobby["gameConfig"]["queueId"] == party["queueId"]
        && lobby["members"].as_array().is_some_and(|ms| {
            ms.iter().any(|m| {
                id(&m["summonerId"])
                    .is_some_and(|sid| id(&local["summonerId"]).as_ref() == Some(&sid))
            })
        })
        && id(&lobby["localMember"]["summonerId"]) == id(&local["summonerId"])
}
fn local_lobby_unchanged(previous: &Value, fresh: &Value, lobby: &Value) -> bool {
    fresh["phase"] == "Lobby"
        && previous["phase"] == "Lobby"
        && previous["lobbyId"].as_str().is_some_and(|s| !s.is_empty())
        && fresh["lobbyId"] == previous["lobbyId"]
        && fresh["puuid"] == previous["puuid"]
        && fresh["server"] == previous["server"]
        && id(&fresh["summonerId"]).is_some()
        && id(&fresh["summonerId"]) == id(&previous["summonerId"])
        && id(&lobby["localMember"]["summonerId"]) == id(&fresh["summonerId"])
        && lobby["members"].as_array().is_some_and(|ms| {
            ms.iter()
                .any(|m| id(&m["summonerId"]) == id(&fresh["summonerId"]))
        })
}
fn selected_login_matches(
    account: &crate::models::account::AccountRecord,
    local: &Value,
    username: Option<&str>,
) -> bool {
    if account.server.trim().is_empty()
        || local["server"].as_str().is_none_or(|server| {
            server.is_empty()
                || crate::services::riot_client::platform_to_server(server.trim())
                    != crate::services::riot_client::platform_to_server(account.server.trim())
        })
        || username.is_some_and(|name| !name.eq_ignore_ascii_case(account.username.trim()))
    {
        return false;
    }
    if crate::models::account::is_valid_puuid(&account.puuid) {
        return local["puuid"].as_str() == Some(account.puuid.as_str());
    }
    if !account.riot_id.trim().is_empty() {
        return local["riotId"]
            .as_str()
            .is_some_and(|name| name.eq_ignore_ascii_case(account.riot_id.trim()));
    }
    username.is_some_and(|name| name.eq_ignore_ascii_case(account.username.trim()))
}
fn invitation_party_unchanged(snapshot: &Value, party: &Value) -> bool {
    let current = &snapshot["active"];
    party["id"].as_str().is_some()
        && party["revision"].as_u64().is_some()
        && current["id"] == party["id"]
        && current["revision"] == party["revision"]
        && current["state"] == "forming"
        && current["leaderId"] == party["leaderId"]
        && is_leader(snapshot, current)
}
async fn invite_new_members(state: &AppState, token: &str, party: &Value) -> Result<(), AppError> {
    use std::sync::{Mutex, OnceLock};
    static SENT: OnceLock<Mutex<std::collections::BTreeMap<String, Instant>>> = OnceLock::new();
    let target = party_server(party).ok_or_else(|| error("Сервер лидера ещё неизвестен"))?;
    let members = accepted(party)?;
    if members.iter().any(|m| {
        m["server"]
            .as_str()
            .is_none_or(|server| crate::services::riot_client::platform_to_server(server) != target)
    }) {
        return Ok(());
    }
    verify_players(state, party).await?;
    let (local, lobby) = read_local(state).await?;
    if local["phase"] != "Lobby" || local["server"] != target {
        return Ok(());
    }
    lobby_matches(party, &lobby, false)?;
    let present: BTreeSet<String> = lobby["members"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| id(&m["summonerId"]))
        .collect();
    let pending: BTreeSet<String> = lobby["invitations"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|i| i["state"] != "Declined" && i["state"] != "Expired")
        .filter_map(|i| id(&i["toSummonerId"]))
        .collect();
    let mut recipients = vec![];
    {
        let mut sent = SENT
            .get_or_init(|| Mutex::new(std::collections::BTreeMap::new()))
            .lock()
            .map_err(|_| error("Приглашения заняты"))?;
        sent.retain(|_, at| at.elapsed() < Duration::from_secs(30));
        for member in members {
            let Some(sid) = id(&member["gameSummonerId"]) else {
                continue;
            };
            let key = format!("{}:{sid}", party["id"].as_str().unwrap_or(""));
            if !present.contains(&sid) && !pending.contains(&sid) && !sent.contains_key(&key) {
                recipients.push((sid, key));
            }
        }
    }
    if recipients.is_empty() {
        return Ok(());
    }
    let latest = league::request(reqwest::Method::GET, "/party", Some(token), None).await?;
    if !invitation_party_unchanged(&latest, party) {
        return Err(error("Состав группы изменился. Приглашения не отправлены"));
    }
    state
        .riot_client
        .lcu_post(
            "/lol-lobby/v2/lobby/invitations",
            &json!(recipients
                .iter()
                .map(|(id, _)| json!({"toSummonerId":id}))
                .collect::<Vec<_>>())
            .to_string(),
        )
        .await?;
    if let Ok(mut sent) = SENT
        .get_or_init(|| Mutex::new(std::collections::BTreeMap::new()))
        .lock()
    {
        for (_, key) in recipients {
            sent.insert(key, Instant::now());
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn party_join_with_account(
    party_id: String,
    username: String,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<(), AppError> {
    use std::sync::atomic::Ordering;
    let _action = ACTION
        .try_lock()
        .map_err(|_| error("Действие с группой уже выполняется"))?;
    if !valid_party_id(&party_id) {
        return Err(error("Неверное приглашение"));
    }
    let token = {
        let _guard = auth_lock().await?;
        league::session(&state).await?
    };
    let snapshot = league::request(reqwest::Method::GET, "/party", Some(&token), None).await?;
    let initial = available_party(&snapshot, &party_id)
        .ok_or_else(|| error("Приглашение больше недоступно"))?;
    let server = party_server(&initial).ok_or_else(|| error("Неизвестен сервер лидера"))?;
    let leader = initial["members"]
        .as_array()
        .and_then(|ms| ms.iter().find(|m| m["id"] == initial["leaderId"]))
        .ok_or_else(|| error("Лидер недоступен"))?;
    if leader["lobbyReady"] != true || leader["online"] != true {
        return Err(error("Дождитесь создания лобби лидером"));
    }
    let account = state
        .accounts
        .load_all()
        .into_iter()
        .find(|a| a.username == username && !a.encrypted_password.is_empty())
        .ok_or_else(|| error("Выбранный аккаунт недоступен"))?;
    if crate::services::riot_client::platform_to_server(account.server.trim()) != server {
        return Err(error("Выберите аккаунт нужного сервера"));
    }
    if !join_target_ready(&initial, &server) {
        return Err(error("Приглашение ещё не готово или группа закрыта"));
    }
    {
        let _guard = auth_lock().await?;
        if let Ok((local, _)) = read_local(&state).await {
            if !matches!(local["phase"].as_str(), Some("None" | "Lobby")) {
                return Err(error(
                    "Сначала отмените поиск или завершите выбор чемпионов и игру",
                ));
            }
        }
    }
    let _login = crate::commands::login::login_local_for_party(username, &state, app)
        .await
        .map_err(error_string)?;
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut sent_accept = false;
    let mut join_attempts = 0;
    let mut cancelled_source = false;
    loop {
        if state.login_cancelled.load(Ordering::Relaxed) {
            return Err(error("Присоединение отменено"));
        }
        if Instant::now() >= deadline {
            return Err(error(
                "Не удалось подтвердить вступление. Откройте пати и повторите присоединение",
            ));
        }
        if let Ok((local, lobby)) = read_local(&state).await {
            let authorized = state
                .riot_client
                .get_authorized_riot_login_username()
                .await
                .ok()
                .filter(|s| !s.is_empty());
            if !selected_login_matches(&account, &local, authorized.as_deref()) {
                return Err(error(
                    "Вход в выбранный аккаунт ещё не подтверждён. Обновите его профиль из клиента",
                ));
            }
            if local["server"] != server {
                return Err(error("League открыта на другом сервере"));
            }
            let current = remote(&token, &local).await?;
            let party = available_party(&current, &party_id)
                .ok_or_else(|| error("Приглашение отменено или группа закрыта"))?;
            if party_server(&party).as_ref() != Some(&server)
                || party["queueId"] != initial["queueId"]
            {
                return Err(error("Лидер изменил сервер или режим"));
            }
            if current["active"]["id"] != party_id {
                if !join_target_ready(&party, &server) {
                    return Err(error("Приглашение больше не готово"));
                }
                if current["active"].is_object() {
                    if cancelled_source
                        || current["me"] != snapshot["me"]
                        || current["active"]["id"] != snapshot["active"]["id"]
                        || !source_lobby_matches(&current, &local, &lobby)
                    {
                        return Err(error("Сначала выйдите из другой группы или лобби вручную"));
                    }
                    let _guard = auth_lock().await?;
                    let (fresh_local, fresh_lobby) = read_local(&state).await?;
                    let latest =
                        league::request(reqwest::Method::GET, "/party", Some(&token), None).await?;
                    let target = available_party(&latest, &party_id)
                        .ok_or_else(|| error("Приглашение отменено"))?;
                    if latest["me"] != current["me"]
                        || latest["active"]["id"] != current["active"]["id"]
                        || latest["active"]["revision"] != current["active"]["revision"]
                        || target != party
                        || !join_target_ready(&target, &server)
                        || fresh_local["puuid"] != local["puuid"]
                        || fresh_local["server"] != local["server"]
                        || !source_lobby_matches(&latest, &fresh_local, &fresh_lobby)
                    {
                        return Err(error(
                            "Группа или League изменились. Повторите присоединение",
                        ));
                    }
                    let source_id = latest["active"]["id"].as_str().unwrap();
                    // Leave once with CAS; an uncertain response must never be retried blindly.
                    league::request(reqwest::Method::POST, &format!("/party/{source_id}/action"), Some(&token),
                        Some(json!({"expectedRevision":latest["active"]["revision"],"action":"leave"}))).await?;
                    cancelled_source = true;
                    if !fresh_lobby.is_null() {
                        let (last_local, last_lobby) = read_local(&state).await?;
                        if last_local["lobbyId"] != fresh_local["lobbyId"]
                            || last_local["puuid"] != fresh_local["puuid"]
                            || !source_lobby_matches(&latest, &last_local, &last_lobby)
                        {
                            return Err(error("Группа покинута; выйдите из лобби League вручную"));
                        }
                        state.riot_client.lcu_delete("/lol-lobby/v2/lobby").await?;
                    }
                    continue;
                }
                if !lobby.is_null() {
                    let _guard = auth_lock().await?;
                    let (fresh, actual) = read_local(&state).await?;
                    let latest =
                        league::request(reqwest::Method::GET, "/party", Some(&token), None).await?;
                    if latest["active"].is_object()
                        || latest["me"] != current["me"]
                        || available_party(&latest, &party_id).as_ref() != Some(&party)
                        || !local_lobby_unchanged(&local, &fresh, &actual)
                    {
                        return Err(error("Лобби изменилось. Повторите присоединение"));
                    }
                    state.riot_client.lcu_delete("/lol-lobby/v2/lobby").await?;
                    continue;
                }
                if join_attempts >= 3 {
                    return Err(error(
                        "Группа меняется слишком быстро. Повторите присоединение",
                    ));
                }
                join_attempts += 1;
                match league::request(
                    reqwest::Method::POST,
                    &format!("/party/{party_id}/action"),
                    Some(&token),
                    Some(json!({"expectedRevision":party["revision"],"action":"join"})),
                )
                .await
                {
                    Ok(_) => {}
                    Err(e) if e.to_string().contains("409") => {
                        tokio::time::sleep(Duration::from_millis(250)).await;
                    }
                    Err(e) => return Err(e),
                }
                continue;
            }
            if !lobby.is_null() {
                let expected = party["members"]
                    .as_array()
                    .and_then(|ms| ms.iter().find(|m| m["id"] == party["leaderId"]))
                    .and_then(|m| id(&m["gameSummonerId"]));
                let joined = lobby["gameConfig"]["queueId"] == party["queueId"]
                    && expected.is_some_and(|leader| {
                        lobby["members"].as_array().is_some_and(|ms| {
                            ms.iter()
                                .any(|m| id(&m["summonerId"]).as_ref() == Some(&leader))
                        })
                    });
                return if joined {
                    Ok(())
                } else {
                    Err(error("Вы уже находитесь в другом лобби League"))
                };
            }
            if !matches!(local["phase"].as_str(), Some("None" | "Lobby")) {
                return Err(error("Сначала завершите текущий поиск или игру"));
            }
            if !sent_accept {
                let invitations =
                    optional_lcu(&state, "/lol-lobby/v2/received-invitations").await?;
                if let Some(invitation) = matching_invitation(&party, &invitations) {
                    sent_accept = true;
                    // No blind retry: after an uncertain response only observe actual lobby membership.
                    let _ = state
                        .riot_client
                        .lcu_post(
                            &format!(
                                "/lol-lobby/v2/received-invitations/{}/accept",
                                urlencoding::encode(&invitation)
                            ),
                            "{}",
                        )
                        .await;
                }
            }
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}
fn error_string(message: String) -> AppError {
    AppError::Custom(message)
}

pub async fn snapshot(state: &AppState) -> Result<Value, AppError> {
    let _guard = auth_lock().await?;
    let started = Instant::now();
    let current = read_local(state).await;
    let mut local = current
        .map(|v| v.0)
        .unwrap_or_else(|_| local_empty(Some("Откройте League и дождитесь загрузки профиля")));
    if matches!(local["phase"].as_str(), Some("None" | "Lobby"))
        && local["lobbyMemberIds"]
            .as_array()
            .is_some_and(|ms| ms.len() < 2)
    {
        crate::services::draft_coordination::set_group_active(false, &state.settings);
    }
    let token = league::session(state).await?;
    let mut data = remote(&token, &local).await?;
    if !data["me"].is_string()
        || !data["invitations"].is_array()
        || !(data["active"].is_null() || data["active"].is_object())
    {
        return Err(error("Сервер вернул несовместимое состояние группы"));
    }
    if data["active"].is_object()
        && data["active"]["leaderId"] != data["me"]
        && local["phase"] == "None"
    {
        if let Ok(invitations) = optional_lcu(state, "/lol-lobby/v2/received-invitations").await {
            local["matchingInvitation"] = json!(matching_invitation(&data["active"], &invitations));
        }
    }
    if data["active"].is_object()
        && is_leader(&data, &data["active"])
        && data["active"]["state"] == "forming"
        && local["phase"] == "Lobby"
    {
        if let Ok(_action) = ACTION.try_lock() {
            let _ = invite_new_members(state, &token, &data["active"]).await;
        }
    }
    if local["phase"] == "Lobby" && data["active"]["state"] == "forming" {
        if let Ok(_action) = ACTION.try_lock() {
            let (_, lobby) = read_local(state).await?;
            sync_positions(state, &data["active"], &data["me"], &lobby, false).await?;
        }
    }
    let together = group_together(&data, &local);
    let status = if state
        .settings
        .load_setting(crate::services::draft_coordination::OPT_IN, true)
    {
        automation_status(&data, &local, together)
    } else {
        "disabled"
    };
    if matches!(status, "solo" | "not-linked" | "disabled" | "empty") {
        crate::services::draft_coordination::set_group_active(false, &state.settings);
    } else if together == Some(true) {
        crate::services::draft_coordination::set_group_active(true, &state.settings);
    }
    data["coordinationStatus"] = json!(status);
    let mut synchronized = true;
    if status == "active" && local["phase"] == "ChampSelect" {
        let own = data["active"]["members"]
            .as_array()
            .and_then(|ms| ms.iter().find(|m| m["id"] == data["me"]))
            .cloned();
        if let Some(own) = own {
            match crate::commands::draft_coordination::refresh_intent(state, &own).await {
                Ok(Some(intent)) => {
                    let action = json!({"expectedRevision":data["active"]["revision"],"action":"draft","draft":intent});
                    match league::request(
                        reqwest::Method::POST,
                        &format!(
                            "/party/{}/action",
                            data["active"]["id"].as_str().unwrap_or("")
                        ),
                        Some(&token),
                        Some(action),
                    )
                    .await
                    {
                        Ok(party) => data["active"] = party,
                        Err(_) => {
                            synchronized = false;
                            crate::services::draft_coordination::unavailable();
                        }
                    }
                }
                Err(_) => {
                    synchronized = false;
                    crate::services::draft_coordination::unavailable();
                }
                Ok(None) => {}
            }
        }
    }
    data["local"] = local;
    data["queues"] = json!(queues(state).await);
    if !synchronized
        || !state
            .settings
            .load_setting(crate::services::draft_coordination::OPT_IN, true)
    {
        data["coordinationStatus"] = json!("waiting");
        crate::services::draft_coordination::unavailable();
        return Ok(data);
    }
    crate::services::draft_coordination::observe_party(&data, started.elapsed());
    if status == "active"
        && data["local"]["phase"] == "ChampSelect"
        && state
            .settings
            .load_setting(crate::services::draft_coordination::OPT_IN, true)
    {
        let own = data["active"]["members"]
            .as_array()
            .and_then(|ms| ms.iter().find(|m| m["id"] == data["me"]));
        if let Some(own) = own {
            if let (Some(session), Some(mode), Some(me)) = (
                own["draft"]["sessionKey"].as_str(),
                own["draft"]["mode"].as_str(),
                data["me"].as_str(),
            ) {
                let plan = crate::services::draft_coordination::coordinate_for_actor(
                    &data["active"]["members"],
                    session,
                    mode,
                    me,
                );
                if let Some(champion) = plan.get(me) {
                    let party = if own["draftChoice"].as_i64() == Some(*champion as i64) {
                        Some(data["active"].clone())
                    } else {
                        league::request(reqwest::Method::POST,&format!("/party/{}/action",data["active"]["id"].as_str().unwrap_or("")),Some(&token),Some(json!({"expectedRevision":data["active"]["revision"],"action":"draftConfirm","championId":champion}))).await.ok()
                    };
                    if let Some(party) = party.filter(|_| {
                        state
                            .settings
                            .load_setting(crate::services::draft_coordination::OPT_IN, true)
                    }) {
                        data["active"] = party.clone();
                        crate::services::draft_coordination::observe_party(
                            &data,
                            started.elapsed(),
                        );
                        crate::services::draft_coordination::confirmed(&party);
                    }
                }
            }
        }
    }
    Ok(data)
}
#[tauri::command]
pub async fn party_snapshot(state: State<'_, AppState>) -> Result<Value, AppError> {
    snapshot(&state).await
}
#[tauri::command]
pub async fn party_friends(state: State<'_, AppState>) -> Result<Value, AppError> {
    let _guard = auth_lock().await?;
    let token = league::session(&state).await?;
    league::request(reqwest::Method::GET, "/friends", Some(&token), None).await
}
#[tauri::command]
pub async fn party_create(
    queue_id: i64,
    invitees: Vec<String>,
    state: State<'_, AppState>,
) -> Result<Value, AppError> {
    let _action = ACTION
        .try_lock()
        .map_err(|_| error("Действие с группой уже выполняется"))?;
    if party_limit(queue_id).is_none_or(|max| invitees.len() + 1 > max)
        || invitees
            .iter()
            .any(|p| !crate::models::account::is_valid_puuid(p))
    {
        return Err(error("Выберите режим и от одного до четырёх друзей LM"));
    }
    let modes = queues(&state).await;
    let selected = modes
        .iter()
        .find(|q| q["id"] == queue_id)
        .ok_or_else(|| error("Режим сейчас недоступен в League"))?;
    if selected["maxPlayers"]
        .as_u64()
        .is_some_and(|max| invitees.len() + 1 > max as usize)
    {
        return Err(error(
            "Для текущего формата режима выбрано слишком много игроков",
        ));
    }
    let _guard = auth_lock().await?;
    let token = league::session(&state).await?;
    let created = league::request(
        reqwest::Method::POST,
        "/party",
        Some(&token),
        Some(json!({"queueId":queue_id,"invitees":invitees})),
    )
    .await?;
    let id = created["id"]
        .as_str()
        .ok_or_else(|| error("Сервер не вернул группу"))?;
    ensure_party_lobby(id, &state)
        .await
        .map_err(|e| error(&format!("Группа создана, но лобби не готово: {e}")))?;
    Ok(created)
}
#[tauri::command]
pub async fn party_action(
    party_id: String,
    request: Value,
    state: State<'_, AppState>,
) -> Result<Value, AppError> {
    let _action = ACTION
        .try_lock()
        .map_err(|_| error("Действие с группой уже выполняется"))?;
    if !valid_party_id(&party_id)
        || !matches!(
            request["action"].as_str(),
            Some("join" | "decline" | "leave" | "update" | "cancel" | "draft" | "draftConfirm")
        )
        || request["expectedRevision"].as_u64().is_none()
    {
        return Err(error("Некорректное действие с группой"));
    }
    let _guard = auth_lock().await?;
    let token = league::session(&state).await?;
    let started = Instant::now();
    let local = read_local(&state)
        .await
        .map(|v| v.0)
        .unwrap_or_else(|_| local_empty(None));
    let data = remote(&token, &local).await?;
    if request["ready"] == true && (local["phase"] != "Lobby" || local["summonerId"] == "") {
        return Err(error(
            "Готовность можно подтвердить после входа в лобби League",
        ));
    }
    let updating = request["action"] == "update";
    let confirming = request["action"] == "draftConfirm";
    if request["action"] == "draft" {
        crate::services::draft_coordination::unavailable();
    }
    let result = league::request(
        reqwest::Method::POST,
        &format!("/party/{party_id}/action"),
        Some(&token),
        Some(request),
    )
    .await?;
    if updating && local["phase"] == "Lobby" {
        let (_, lobby) = read_local(&state).await?;
        sync_positions(&state, &result, &data["me"], &lobby, false).await?;
    }
    if confirming {
        let mut observed = data;
        observed["active"] = result.clone();
        observed["local"] = local;
        crate::services::draft_coordination::observe_party(&observed, started.elapsed());
        crate::services::draft_coordination::confirmed(&result);
    }
    Ok(result)
}
fn position_preferences(member: &Value) -> Option<Value> {
    let primary = member["role"].as_str()?;
    if primary == "fill" {
        return Some(json!({"firstPreference":"FILL","secondPreference":"UNSELECTED"}));
    }
    let secondary = member["secondaryRole"].as_str()?;
    let roles = ["top", "jungle", "middle", "bottom", "utility"];
    (roles.contains(&primary) && roles.contains(&secondary) && primary != secondary)
        .then(|| json!({"firstPreference":primary.to_uppercase(),"secondPreference":secondary.to_uppercase()}))
}
fn lobby_positions_match(party: &Value, lobby: &Value) -> Result<(), AppError> {
    if lobby["gameConfig"]["showPositionSelector"] != true {
        return Ok(());
    }
    for member in accepted(party)? {
        let expected =
            position_preferences(member).ok_or_else(|| error("Выберите две разные роли"))?;
        let actual = lobby["members"]
            .as_array()
            .and_then(|ms| {
                ms.iter()
                    .find(|m| id(&m["summonerId"]) == id(&member["gameSummonerId"]))
            })
            .ok_or_else(|| error("Состав League изменился"))?;
        if actual["firstPositionPreference"] != expected["firstPreference"]
            || actual["secondPositionPreference"] != expected["secondPreference"]
        {
            return Err(error(
                "Дождитесь синхронизации ролей всех участников с League",
            ));
        }
    }
    Ok(())
}
async fn sync_positions(
    state: &AppState,
    party: &Value,
    me: &Value,
    lobby: &Value,
    strict: bool,
) -> Result<(), AppError> {
    if lobby["gameConfig"]["showPositionSelector"] != true {
        return Ok(());
    }
    let member = party["members"]
        .as_array()
        .and_then(|ms| ms.iter().find(|m| m["id"] == *me))
        .ok_or_else(|| error("Участник группы не найден"))?;
    let Some(preferences) = position_preferences(member) else {
        return if strict {
            Err(error("Выберите две разные роли"))
        } else {
            Ok(())
        };
    };
    if lobby["localMember"]["firstPositionPreference"] == preferences["firstPreference"]
        && lobby["localMember"]["secondPositionPreference"] == preferences["secondPreference"]
    {
        return Ok(());
    }
    state
        .riot_client
        .lcu_put(
            "/lol-lobby/v2/lobby/members/localMember/position-preferences",
            &preferences.to_string(),
        )
        .await?;
    let actual = optional_lcu(state, "/lol-lobby/v2/lobby").await?;
    if actual["localMember"]["firstPositionPreference"] != preferences["firstPreference"]
        || actual["localMember"]["secondPositionPreference"] != preferences["secondPreference"]
    {
        return Err(error("League не подтвердила выбранные роли"));
    }
    Ok(())
}
async fn verify_players(state: &AppState, party: &Value) -> Result<(), AppError> {
    for member in accepted(party)? {
        let summoner = id(&member["gameSummonerId"])
            .ok_or_else(|| error("Неизвестен игровой аккаунт участника"))?;
        let value = optional_lcu(state, &format!("/lol-summoner/v1/summoners/{summoner}")).await?;
        let expected = member["gameRiotId"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| error("Неизвестен Riot ID участника"))?;
        if riot_id(&value).map(|v| v.to_lowercase()) != Some(expected.to_lowercase()) {
            return Err(error(
                "Игровой аккаунт участника изменился. Обновите готовность",
            ));
        }
    }
    Ok(())
}
#[tauri::command]
pub async fn party_lobby(party_id: String, state: State<'_, AppState>) -> Result<(), AppError> {
    let _action = ACTION
        .try_lock()
        .map_err(|_| error("Действие с группой уже выполняется"))?;
    let _guard = auth_lock().await?;
    ensure_party_lobby(&party_id, &state).await
}
async fn ensure_party_lobby(party_id: &str, state: &AppState) -> Result<(), AppError> {
    let token = league::session(&state).await?;
    let (local, _) = read_local(&state).await?;
    let data = remote(&token, &local).await?;
    let party = active(&data, &party_id)?;
    if !is_leader(&data, party)
        || party["state"] != "forming"
        || !matches!(local["phase"].as_str(), Some("None" | "Lobby"))
    {
        return Err(error("Создать лобби может лидер до начала поиска"));
    }
    verify_players(&state, party).await?;
    let latest = league::request(reqwest::Method::GET, "/party", Some(&token), None).await?;
    if active(&latest, &party_id)?["revision"] != party["revision"] {
        return Err(error("Состав группы изменился. Обновите приглашения"));
    }
    let (fresh, current_lobby) = read_local(&state).await?;
    if fresh["summonerId"] != local["summonerId"]
        || !matches!(fresh["phase"].as_str(), Some("None" | "Lobby"))
    {
        return Err(error("Состояние League изменилось. Лобби не создавалось"));
    }
    let mut lobby = current_lobby;
    if lobby.is_null() {
        state
            .riot_client
            .lcu_post(
                "/lol-lobby/v2/lobby",
                &json!({"queueId":party["queueId"]}).to_string(),
            )
            .await?;
        lobby = optional_lcu(&state, "/lol-lobby/v2/lobby").await?;
    }
    lobby_matches(party, &lobby, false)?;
    let present: BTreeSet<String> = lobby["members"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|m| id(&m["summonerId"]))
        .collect();
    let invites: Vec<Value> = accepted(party)?
        .iter()
        .filter_map(|m| id(&m["gameSummonerId"]))
        .filter(|sid| !present.contains(sid))
        .map(|sid| json!({"toSummonerId":sid}))
        .collect();
    if !invites.is_empty() {
        state
            .riot_client
            .lcu_post(
                "/lol-lobby/v2/lobby/invitations",
                &json!(invites).to_string(),
            )
            .await?;
    }
    Ok(())
}
#[tauri::command]
pub async fn party_accept_lobby(
    party_id: String,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    let _action = ACTION
        .try_lock()
        .map_err(|_| error("Действие с группой уже выполняется"))?;
    let _guard = auth_lock().await?;
    let token = league::session(&state).await?;
    let (local, lobby) = read_local(&state).await?;
    let data = remote(&token, &local).await?;
    let party = active(&data, &party_id)?;
    if !lobby.is_null() || local["phase"] != "None" {
        return Err(error("Сначала выйдите из другого лобби League"));
    }
    let invitations = optional_lcu(&state, "/lol-lobby/v2/received-invitations").await?;
    let invitation = matching_invitation(party, &invitations)
        .ok_or_else(|| error("Ожидаем приглашение League от лидера группы"))?;
    state
        .riot_client
        .lcu_post(
            &format!(
                "/lol-lobby/v2/received-invitations/{}/accept",
                urlencoding::encode(&invitation)
            ),
            "{}",
        )
        .await?;
    Ok(())
}
fn search_outcome(phase: &Value) -> Option<bool> {
    let phase = phase.as_str()?;
    if matches!(
        phase,
        "Matchmaking" | "ReadyCheck" | "ChampSelect" | "GameStart" | "InProgress" | "Reconnect"
    ) {
        Some(true)
    } else if matches!(phase, "Lobby" | "None") {
        Some(false)
    } else {
        None
    }
}
#[tauri::command]
pub async fn party_queue(
    party_id: String,
    expected_revision: u64,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    let _action = ACTION
        .try_lock()
        .map_err(|_| error("Действие с группой уже выполняется"))?;
    let _guard = auth_lock().await?;
    let token = league::session(&state).await?;
    let (local, lobby) = read_local(&state).await?;
    let data = remote(&token, &local).await?;
    let party = active(&data, &party_id)?;
    if !is_leader(&data, party) || local["phase"] != "Lobby" {
        return Err(error("Начать поиск может лидер в лобби League"));
    }
    lobby_matches(party, &lobby, true)?;
    sync_positions(&state, party, &data["me"], &lobby, true).await?;
    verify_players(&state, party).await?;
    let changed = league::request(
        reqwest::Method::POST,
        &format!("/party/{party_id}/action"),
        Some(&token),
        Some(json!({"expectedRevision":expected_revision,"action":"beginQueue"})),
    )
    .await?;
    let (fresh, lobby) = match read_local(&state).await {
        Ok(value) => value,
        Err(e) => {
            // No search POST was sent, so this attempt is safe to roll back.
            let _ = league::request(reqwest::Method::POST, &format!("/party/{party_id}/action"), Some(&token), Some(json!({"expectedRevision":changed["revision"],"action":"finishQueue","success":false}))).await;
            return Err(e);
        }
    };
    let result = if fresh["phase"] == "Lobby" {
        match lobby_matches(&changed, &lobby, true)
            .and_then(|_| lobby_positions_match(&changed, &lobby))
        {
            Ok(()) => state
                .riot_client
                .lcu_post("/lol-lobby/v2/lobby/matchmaking/search", "{}")
                .await
                .map(|_| ()),
            Err(e) => Err(e),
        }
    } else {
        Err(error("Состояние League изменилось. Поиск не запускался"))
    };
    let actual: Value = serde_json::from_str(
        &state
            .riot_client
            .lcu_get("/lol-gameflow/v1/gameflow-phase")
            .await
            .unwrap_or_else(|_| "null".into()),
    )
    .unwrap_or(Value::Null);
    let confirmed = search_outcome(&actual)
        .ok_or_else(|| error("Результат поиска не подтверждён. Проверьте League перед повтором"))?;
    // Reconcile an uncertain response against LCU; never blindly repeat the search POST.
    let ack=league::request(reqwest::Method::POST,&format!("/party/{party_id}/action"),Some(&token),Some(json!({"expectedRevision":changed["revision"],"action":"finishQueue","success":confirmed}))).await;
    if confirmed {
        ack?;
        return Ok(());
    }
    ack?;
    result?;
    Err(error("League не подтвердила начало поиска"))
}
#[tauri::command]
pub async fn party_cancel_queue(
    party_id: String,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    let _action = ACTION
        .try_lock()
        .map_err(|_| error("Действие с группой уже выполняется"))?;
    let _guard = auth_lock().await?;
    let token = league::session(&state).await?;
    let (local, lobby) = read_local(&state).await?;
    let data = remote(&token, &local).await?;
    let party = active(&data, &party_id)?;
    if !is_leader(&data, party) || !matches!(local["phase"].as_str(), Some("Matchmaking" | "Lobby"))
    {
        return Err(error("Отменить поиск может лидер до принятия матча"));
    }
    lobby_matches(party, &lobby, false)?;
    if local["phase"] == "Matchmaking" {
        state
            .riot_client
            .lcu_delete("/lol-lobby/v2/lobby/matchmaking/search")
            .await?;
    }
    let phase = state
        .riot_client
        .lcu_get("/lol-gameflow/v1/gameflow-phase")
        .await?;
    if phase.trim_matches('"') != "Lobby" {
        return Err(error("League ещё не подтвердила отмену поиска"));
    }
    league::request(
        reqwest::Method::POST,
        &format!("/party/{party_id}/action"),
        Some(&token),
        Some(json!({"expectedRevision":party["revision"],"action":"finishQueue","success":false})),
    )
    .await?;
    Ok(())
}
pub async fn run(app: tauri::AppHandle) {
    loop {
        let state = app.state::<AppState>();
        let delay = match snapshot(&state).await {
            Ok(data) => {
                let active = data["active"].is_object();
                let _ = app.emit("party-changed", &data);
                if active
                    || data["invitations"]
                        .as_array()
                        .is_some_and(|rows| !rows.is_empty())
                {
                    1
                } else {
                    2
                }
            }
            Err(_) => {
                crate::services::draft_coordination::unavailable();
                5
            }
        };
        tokio::time::sleep(Duration::from_secs(delay)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn group() -> Value {
        json!({"queueId":450,"members":[{"id":"a","accepted":true,"gameSummonerId":"1","gameRiotId":"A#RU"},{"id":"b","accepted":true,"gameSummonerId":"2","gameRiotId":"B#RU"}]})
    }
    fn lobby() -> Value {
        json!({"gameConfig":{"queueId":450,"maxLobbySize":5},"localMember":{"isLeader":true},"canStartActivity":true,"members":[{"summonerId":1},{"summonerId":2}]})
    }
    #[test]
    fn party_rank_uses_only_reported_league_rank_and_never_defaults() {
        let solo =
            json!({"queueType":"RANKED_SOLO_5x5","tier":"GOLD","division":"II","leaguePoints":42});
        let flex =
            json!({"queueType":"RANKED_FLEX_SR","tier":"SILVER","division":"I","leaguePoints":9});
        assert_eq!(
            rank_display(&json!({"queues":[flex, solo]})).as_deref(),
            Some("GOLD II 42LP")
        );
        for stats in [
            Value::Null,
            json!({"queues":[]}),
            json!({"queues":[{"queueType":"RANKED_TFT","tier":"GOLD","division":"II","leaguePoints":42}]}),
            json!({"queues":[{"queueType":"RANKED_SOLO_5x5","tier":"GOLD","division":"II"}]}),
        ] {
            assert_eq!(rank_display(&stats), None);
        }
    }
    #[test]
    fn lobby_requires_exact_group_queue_leadership_and_membership() {
        assert!(lobby_matches(&group(), &lobby(), true).is_ok());
        for (field, value) in [
            ("members", json!([{"summonerId":1},{"summonerId":3}])),
            ("members", json!([{"summonerId":1},{"summonerId":1}])),
            ("members", json!([{"summonerId":1}])),
            ("localMember", json!({"isLeader":false})),
            ("gameConfig", json!({"queueId":420})),
            ("canStartActivity", json!(false)),
        ] {
            let mut bad = lobby();
            bad[field] = value;
            assert!(lobby_matches(&group(), &bad, true).is_err());
        }
    }
    #[test]
    fn partial_lobby_can_invite_but_cannot_start_search() {
        let mut current = lobby();
        current["members"] = json!([{"summonerId":1}]);
        assert!(lobby_matches(&group(), &current, false).is_ok());
        assert!(lobby_matches(&group(), &current, true).is_err());
    }
    #[test]
    fn invitations_must_be_from_the_expected_live_leader_and_queue() {
        let mut p = group();
        p["leaderId"] = json!("a");
        p["members"][0]["online"] = json!(true);
        let invites = json!([{"invitationId":"wanted","fromSummonerId":1,"state":"Pending","canAcceptInvitation":true,"gameConfig":{"queueId":450}}]);
        assert_eq!(matching_invitation(&p, &invites).as_deref(), Some("wanted"));
        for (field, value) in [
            ("fromSummonerId", json!(3)),
            ("invitationId", json!("../other")),
            ("gameConfig", json!({"queueId":420})),
            ("canAcceptInvitation", json!(false)),
        ] {
            let mut bad = invites.clone();
            bad[0][field] = value;
            assert!(matching_invitation(&p, &bad).is_none());
        }
    }
    #[test]
    fn identifiers_do_not_round_64_bit_summoner_ids_or_allow_route_injection() {
        assert_eq!(
            id(&json!(18446744073709551614u64)).as_deref(),
            Some("18446744073709551614")
        );
        for value in [
            json!(0),
            json!(-1),
            json!(1.1),
            json!("../1"),
            json!("0"),
            Value::Null,
        ] {
            assert!(id(&value).is_none());
        }
        assert!(valid_party_id(&"a".repeat(32)));
        assert!(!valid_party_id("../other"));
    }
}

#[cfg(test)]
mod queue_outcome_tests {
    use super::*;
    #[test]
    fn uncertain_search_never_becomes_negative_ack() {
        for phase in [
            Value::Null,
            json!("Unknown"),
            json!("WaitingForStats"),
            json!({}),
        ] {
            assert_eq!(search_outcome(&phase), None);
        }
        for phase in ["Matchmaking", "ReadyCheck", "ChampSelect", "InProgress"] {
            assert_eq!(search_outcome(&json!(phase)), Some(true));
        }
        assert_eq!(search_outcome(&json!("Lobby")), Some(false));
    }
}

#[cfg(test)]
mod party_size_tests {
    use super::*;
    #[test]
    fn requested_queue_limits_are_enforced_before_lcu_writes() {
        assert!(party_size_allowed(420, 2));
        assert!(!party_size_allowed(420, 3));
        assert!(party_size_allowed(1750, 3));
        assert!(!party_size_allowed(1750, 4));
        for n in [1, 2, 3, 5] {
            assert!(party_size_allowed(440, n));
        }
        assert!(!party_size_allowed(440, 4));
        for q in [400, 450, 2400] {
            assert!(party_size_allowed(q, 5));
            assert!(!party_size_allowed(q, 6));
        }
        assert!(!party_size_allowed(9999, 1));
        assert!(!party_size_allowed(450, 0));
    }
}

#[cfg(test)]
mod automatic_group_tests {
    use super::*;
    #[test]
    fn saved_lm_party_is_not_a_real_group_and_solo_clears_queued_binding() {
        let summary = json!({"mode":"aram","autoPick":true,"autoBan":false,"preferences":{"pick":[{"championId":99,"state":"must"}],"ban":[]}});
        let mut data = json!({"me":"a","active":{"id":"test-auto-party","members":[{"id":"a","accepted":true,"online":true,"gameSummonerId":"1","automation":summary},{"id":"b","accepted":true,"online":true,"gameSummonerId":"2","automation":summary}]}});
        let mut local =
            json!({"summonerId":"1","phase":"Lobby","lobbyMemberIds":["1"],"automation":summary});
        assert_eq!(group_together(&data, &local), Some(false));
        assert_eq!(automation_status(&data, &local, Some(false)), "solo");
        local["lobbyMemberIds"] = json!(["1", "2"]);
        assert_eq!(group_together(&data, &local), Some(true));
        assert_eq!(automation_status(&data, &local, Some(true)), "active");
        data["active"]["members"][1]["automation"]["autoPick"] = json!(false);
        assert_eq!(automation_status(&data, &local, Some(true)), "disabled");
        data["active"]["members"][1]["automation"]["autoPick"] = json!(true);
        data["active"]["members"][1]["automation"]["preferences"]["pick"] = json!([]);
        assert_eq!(automation_status(&data, &local, Some(true)), "empty");
        local["phase"] = json!("ChampSelect");
        local["lobbyMemberIds"] = json!([]);
        assert_eq!(group_together(&data, &local), Some(true));
        local["phase"] = json!("Lobby");
        local["lobbyMemberIds"] = json!(["1"]);
        assert_eq!(group_together(&data, &local), Some(false));
        local["phase"] = json!("ChampSelect");
        local["lobbyMemberIds"] = json!([]);
        assert_eq!(group_together(&data, &local), None);
    }
}

#[cfg(test)]
mod party_join_identity_tests {
    use super::*;
    use crate::models::account::AccountRecord;
    #[test]
    fn account_switch_requires_positive_selected_identity_and_never_only_server() {
        let mut account = AccountRecord {
            username: "owner".into(),
            server: "RU".into(),
            ..Default::default()
        };
        let mut local = json!({"puuid":"a".repeat(78),"riotId":"Owner#RU1","server":"RU"});
        assert!(!selected_login_matches(&account, &local, None));
        assert!(selected_login_matches(&account, &local, Some("OWNER")));
        assert!(!selected_login_matches(
            &account,
            &local,
            Some("someone-else")
        ));
        account.puuid = "a".repeat(78);
        assert!(selected_login_matches(&account, &local, None));
        local["puuid"] = json!("b".repeat(78));
        assert!(!selected_login_matches(&account, &local, Some("owner")));
        account.puuid.clear();
        account.riot_id = "Owner#RU1".into();
        assert!(selected_login_matches(&account, &local, None));
        local["riotId"] = json!("Else#RU1");
        assert!(!selected_login_matches(&account, &local, None));
    }
    #[test]
    fn switching_requires_own_accepted_forming_membership() {
        let source = json!({"me":"me","active":{"id":"a".repeat(32),"revision":1,"leaderId":"me","state":"forming","members":[{"id":"me","accepted":true},{"id":"pending","accepted":false}]}});
        assert!(switchable_party(&source));
        for (field, value) in [("state", json!("queuing")), ("revision", Value::Null)] {
            let mut bad = source.clone();
            bad["active"][field] = value;
            assert!(!switchable_party(&bad));
        }
        let mut bad = source.clone();
        bad["active"]["members"][1]["accepted"] = json!(true);
        assert!(switchable_party(&bad));
        bad = source;
        bad["active"]["members"][0]["id"] = json!("other");
        assert!(!switchable_party(&bad));
    }

    #[test]
    fn selected_identity_also_requires_server_proof() {
        let account = AccountRecord {
            username: "owner".into(),
            server: "EUW".into(),
            puuid: "a".repeat(78),
            ..Default::default()
        };
        for server in [Value::Null, json!(""), json!("RU")] {
            assert!(!selected_login_matches(
                &account,
                &json!({"puuid":account.puuid,"server":server}),
                Some("owner")
            ));
        }
        assert!(selected_login_matches(
            &account,
            &json!({"puuid":account.puuid,"server":"EUW1"}),
            Some("owner")
        ));
    }

    #[test]
    fn switching_requires_exact_source_lobby_and_idle_phase() {
        let source = json!({"me":"me","active":{"id":"a".repeat(32),"revision":1,"leaderId":"me","state":"forming","queueId":450,"members":[{"id":"me","accepted":true,"gameSummonerId":"1","lobbyId":"source-lobby"}]}});
        let local =
            json!({"phase":"Lobby","lobbyId":"source-lobby","lobbyLeader":true,"summonerId":"1"});
        let lobby = json!({"gameConfig":{"queueId":450},"localMember":{"isLeader":true,"summonerId":1},"members":[{"summonerId":1}]});
        assert!(source_lobby_matches(&source, &local, &lobby));
        let mut shared = lobby.clone();
        shared["members"] = json!([{"summonerId":1},{"summonerId":2}]);
        shared["localMember"]["isLeader"] = json!(false);
        assert!(source_lobby_matches(&source, &local, &shared));
        for (field, value) in [
            ("phase", json!("Matchmaking")),
            ("lobbyId", json!("other")),
            ("summonerId", json!("2")),
        ] {
            let mut bad = local.clone();
            bad[field] = value;
            assert!(!source_lobby_matches(&source, &bad, &lobby));
        }
        for (field, value) in [
            ("gameConfig", json!({"queueId":420})),
            ("localMember", json!({"isLeader":false,"summonerId":2})),
        ] {
            let mut bad = lobby.clone();
            bad[field] = value;
            assert!(!source_lobby_matches(&source, &local, &bad));
        }
    }

    #[test]
    fn recipient_uses_original_lm_invitation_across_different_current_game_accounts() {
        let p =
            json!({"id":"wanted","leaderId":"leader","members":[{"id":"leader","server":"EUW1"}]});
        assert_eq!(party_server(&p).as_deref(), Some("EUW"));
        let data = json!({"active":null,"invitations":[p]});
        assert!(available_party(&data, "wanted").is_some());
        assert!(available_party(&data, "other").is_none());
    }
}

#[cfg(test)]
mod invitation_revision_tests {
    use super::*;

    #[test]
    fn invitations_require_the_same_forming_party_and_local_leader() {
        let party = json!({"id":"party", "revision":7, "state":"forming", "leaderId":"me"});
        let snapshot = json!({"me":"me", "active":party});
        assert!(invitation_party_unchanged(&snapshot, &party));
        for (field, value) in [
            ("id", json!("other")),
            ("revision", json!(8)),
            ("revision", Value::Null),
            ("state", json!("cancelled")),
            ("leaderId", json!("other")),
        ] {
            let mut changed = snapshot.clone();
            changed["active"][field] = value;
            assert!(!invitation_party_unchanged(&changed, &party), "{field}");
        }
        let mut gone = snapshot.clone();
        gone["active"] = Value::Null;
        assert!(!invitation_party_unchanged(&gone, &party));
        let mut other_owner = snapshot;
        other_owner["me"] = json!("other");
        assert!(!invitation_party_unchanged(&other_owner, &party));
    }
}

#[cfg(test)]
mod role_preferences_tests {
    use super::*;
    #[test]
    fn exact_two_role_lcu_payload_never_guesses_secondary() {
        for (primary, secondary) in [
            ("top", "jungle"),
            ("jungle", "top"),
            ("middle", "utility"),
            ("bottom", "middle"),
        ] {
            assert_eq!(
                position_preferences(&json!({"role":primary,"secondaryRole":secondary})),
                Some(
                    json!({"firstPreference":primary.to_uppercase(),"secondPreference":secondary.to_uppercase()})
                )
            );
        }
        assert_eq!(
            position_preferences(&json!({"role":"fill","secondaryRole":""})),
            Some(json!({"firstPreference":"FILL","secondPreference":"UNSELECTED"}))
        );
        for member in [
            json!({"role":"top"}),
            json!({"role":"top","secondaryRole":"top"}),
            json!({"role":"top","secondaryRole":"fill"}),
            json!({"role":"","secondaryRole":"jungle"}),
        ] {
            assert_eq!(position_preferences(&member), None);
        }
    }
    #[test]
    fn queue_checks_actual_lcu_pairs_without_a_ready_flag() {
        let party = json!({"members":[{"accepted":true,"gameSummonerId":"1","role":"top","secondaryRole":"jungle"},{"accepted":true,"gameSummonerId":"2","role":"fill","secondaryRole":""}]});
        let mut lobby = json!({"gameConfig":{"showPositionSelector":true},"members":[{"summonerId":1,"firstPositionPreference":"TOP","secondPositionPreference":"JUNGLE"},{"summonerId":2,"firstPositionPreference":"FILL","secondPositionPreference":"UNSELECTED"}]});
        assert!(lobby_positions_match(&party, &lobby).is_ok());
        for second in ["TOP", "FILL", "UNSELECTED", "MIDDLE"] {
            lobby["members"][0]["secondPositionPreference"] = json!(second);
            assert!(lobby_positions_match(&party, &lobby).is_err());
        }
    }
    #[test]
    fn actual_outcome_requires_own_unique_game_identity() {
        let own = json!({"puuid":"own","summonerId":1});
        let mut session = json!({"gameData":{"gameId":123,"teamOne":[{"puuid":"own","summonerId":1,"championId":99}],"teamTwo":[{"puuid":"peer","championId":1}]}});
        assert_eq!(game_outcome(&session, &own), ("123".into(), 99));
        session["gameData"]["teamOne"][0]["puuid"] = json!("other");
        assert_eq!(game_outcome(&session, &own), (String::new(), 0));
    }
    #[test]
    fn lobby_leave_pins_identity_fingerprint_and_idle_phase_without_solo_restriction() {
        let local = json!({"phase":"Lobby","lobbyId":"fingerprint","puuid":"own","server":"RU","summonerId":"1"});
        let lobby = json!({"localMember":{"summonerId":1,"isLeader":false},"members":[{"summonerId":1},{"summonerId":2}]});
        assert!(local_lobby_unchanged(&local, &local, &lobby));
        for (key, value) in [
            ("phase", json!("ChampSelect")),
            ("phase", json!("InProgress")),
            ("lobbyId", json!("other")),
            ("summonerId", json!("2")),
            ("puuid", json!("other")),
        ] {
            let mut fresh = local.clone();
            fresh[key] = value;
            assert!(!local_lobby_unchanged(&local, &fresh, &lobby));
        }
    }
}

#[cfg(test)]
mod mode_capacity_tests {
    use super::*;
    #[test]
    fn arena_limit_follows_active_format_without_hardcoding_eighteen_bans() {
        assert_eq!(
            queue_capacity(&json!({"id":1700,"numPlayersPerTeam":2})),
            Some(2)
        );
        assert_eq!(
            queue_capacity(&json!({"id":1700,"numPlayersPerTeam":3})),
            Some(3)
        );
        assert_eq!(
            queue_capacity(&json!({"id":420,"maximumParticipantListSize":2})),
            Some(2)
        );
        assert_eq!(
            queue_capacity(&json!({"id":450,"maximumParticipantListSize":0})),
            Some(5)
        );
        assert_eq!(
            queue_capacity(&json!({"id":1700,"numPlayersPerTeam":99})),
            Some(3)
        );
    }
}
