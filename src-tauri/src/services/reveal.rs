#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::Value;
use tokio::sync::{Mutex, RwLock};

use crate::error::AppError;
use crate::models::player::{MatchSummary, PlayerInfo, RevealSnapshot};
use crate::services::file_logger::FileLogger;
use crate::services::riot_client::RiotClientService;
use crate::services::settings::SettingsService;

const REVEAL_API_KEY_SETTING: &str = "RevealApiKey";
const REVEAL_REGION_SETTING: &str = "RevealRegion";

const CHAMP_SELECT_SESSION: &str = "/lol-champ-select/v1/session";
const GAMEFLOW_SESSION: &str = "/lol-gameflow/v1/session";
const CURRENT_SUMMONER: &str = "/lol-summoner/v1/current-summoner";
const LOBBY_MEMBERS: &str = "/lol-lobby/v2/lobby/members";
const CHAT_CONVERSATIONS: &str = "/lol-chat/v1/conversations";

/// Ранги кешируются: без кеша каждое обновление списка тратит 10 запросов
/// к Riot API (dev-ключ — 100 запросов за 2 минуты).
const RANKED_CACHE_TTL: Duration = Duration::from_secs(600);
const RANKED_CACHE_MAX: usize = 300;
/// PUUID в LCU — 78 символов. Короткие строки — заглушки, на них Riot API
/// отвечает 400 и сжигает квоту.
const MIN_PUUID_LEN: usize = 30;

const MATCH_CACHE_TTL: Duration = Duration::from_secs(300);
const MATCH_CACHE_MAX: usize = 100;

/// Локализованный Riot ID по PUUID меняется редко, а запрос стоит квоты.
const NAME_CACHE_TTL: Duration = Duration::from_secs(1800);
const NAME_CACHE_MAX: usize = 500;

const QUEUE_RANKED_SOLO: i64 = 420;
const QUEUE_RANKED_FLEX: i64 = 440;
const QUEUE_RANKED_SOLO_OLD: i64 = 410;

/// Один слот выбора чемпионов, разобранный из ответа LCU.
#[derive(Debug, Clone, Default, PartialEq)]
struct Slot {
    cell_id: i32,
    team: i32,
    champion_id: i32,
    summoner_id: i64,
    puuid: String,
    game_name: String,
    tag_line: String,
    position: String,
}

impl Slot {
    fn has_any_id(&self) -> bool {
        self.summoner_id != 0 || !self.puuid.is_empty() || !self.game_name.is_empty()
    }
}

/// Идентичность игрока, собранная из любого источника LCU.
#[derive(Debug, Clone, Default, PartialEq)]
struct Identity {
    game_name: String,
    tag_line: String,
    puuid: String,
    summoner_id: i64,
    level: i32,
    profile_icon_id: i32,
}

#[derive(Debug, Clone, Default, PartialEq)]
struct RankedInfo {
    tier: String,
    rank: String,
    league_points: i32,
    wins: i32,
    losses: i32,
}

/// Сколько данных отдал каждый источник — пишется в лог один раз на игру.
#[derive(Debug, Clone, Copy, Default)]
struct SourceStats {
    slots: usize,
    slots_with_ids: usize,
    known_identities: usize,
}

pub struct RevealService {
    riot_client: Arc<RiotClientService>,
    settings: Arc<SettingsService>,
    logger: Arc<FileLogger>,
    api_key: RwLock<Option<String>>,
    region: RwLock<String>,
    http_client: reqwest::Client,
    ranked_cache: Mutex<HashMap<String, (Instant, RankedInfo)>>,
    match_cache: Mutex<HashMap<String, (Instant, Vec<MatchSummary>)>>,
    name_cache: Mutex<HashMap<String, (Instant, Option<(String, String)>)>>,
    last_session_log: Mutex<Option<(i64, Instant)>>,
    last_summary_log: Mutex<Option<String>>,
    last_chat_log: Mutex<Option<String>>,
}

impl RevealService {
    pub fn new(
        riot_client: Arc<RiotClientService>,
        settings: Arc<SettingsService>,
        logger: Arc<FileLogger>,
    ) -> Self {
        let saved_key: String = settings.load_setting(REVEAL_API_KEY_SETTING, String::new());
        let saved_region: String = settings.load_setting(REVEAL_REGION_SETTING, "euw1".to_string());

        let api_key = if saved_key.is_empty() {
            RwLock::new(None)
        } else {
            RwLock::new(Some(saved_key))
        };

        Self {
            riot_client,
            settings,
            logger,
            api_key,
            region: RwLock::new(saved_region),
            http_client: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .unwrap(),
            ranked_cache: Mutex::new(HashMap::new()),
            match_cache: Mutex::new(HashMap::new()),
            name_cache: Mutex::new(HashMap::new()),
            last_session_log: Mutex::new(None),
            last_summary_log: Mutex::new(None),
            last_chat_log: Mutex::new(None),
        }
    }

    pub async fn get_api_config(&self) -> (String, String) {
        let key = self.api_key.read().await.clone().unwrap_or_default();
        let region = self.region.read().await.clone();
        (key, region)
    }

    pub async fn set_api_configuration(&self, api_key: &str, region: &str) {
        *self.api_key.write().await = if api_key.is_empty() {
            None
        } else {
            Some(api_key.to_string())
        };
        *self.region.write().await = region.to_string();
        let _ = self
            .settings
            .save_setting(REVEAL_API_KEY_SETTING, &api_key.to_string());
        let _ = self
            .settings
            .save_setting(REVEAL_REGION_SETTING, &region.to_string());
        // Регион мог измениться — старые ранги к другому серверу не относятся.
        self.ranked_cache.lock().await.clear();
        self.match_cache.lock().await.clear();
    }

    pub async fn reload_from_settings(&self) {
        let saved_key: String = self
            .settings
            .load_setting(REVEAL_API_KEY_SETTING, String::new());
        let saved_region: String = self
            .settings
            .load_setting(REVEAL_REGION_SETTING, "euw1".to_string());
        *self.api_key.write().await = if saved_key.is_empty() {
            None
        } else {
            Some(saved_key)
        };
        *self.region.write().await = saved_region;
    }

    async fn riot_api_get(&self, url: &str) -> Result<String, AppError> {
        let key = {
            let guard = self.api_key.read().await;
            guard.clone()
        }
        .ok_or_else(|| AppError::Custom("Riot API ключ не задан".to_string()))?;

        let resp = self
            .http_client
            .get(url)
            .header("X-Riot-Token", key)
            .send()
            .await?;

        let status = resp.status();
        let retry_after = resp
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok());
        let body = resp.text().await.map_err(AppError::Http)?;

        if status.is_success() {
            return Ok(body);
        }

        let message = match status.as_u16() {
            401 => "Riot API: ключ не принят (401). Проверь ключ.".to_string(),
            403 => "Riot API: ключ просрочен или отозван (403). Dev-ключи Riot действуют 24 часа — обнови его на developer.riotgames.com.".to_string(),
            404 => "Riot API: данные не найдены (404).".to_string(),
            429 => match retry_after {
                Some(secs) => format!("Riot API: лимит запросов (429). Повтори через {} c.", secs),
                None => "Riot API: лимит запросов (429).".to_string(),
            },
            code => format!("Riot API: HTTP {}", code),
        };
        Err(AppError::Custom(message))
    }

    pub async fn test_api_key(&self) -> Result<(bool, String), AppError> {
        let region = self.region.read().await.clone();
        let url = format!(
            "https://{}.api.riotgames.com/lol/status/v4/platform-data",
            region
        );

        match self.riot_api_get(&url).await {
            Ok(_) => Ok((true, "API ключ валиден".to_string())),
            Err(e) => Ok((false, e.to_string())),
        }
    }

    /// Одна точка входа для страницы «Разведка».
    ///
    /// Возвращает снимок состояния, а не ошибку, когда клиент закрыт или мы не
    /// в выборе чемпионов: UI должен объяснить ситуацию, а не показать пустоту.
    pub async fn get_teams_info(&self) -> Result<RevealSnapshot, AppError> {
        let api_configured = self.api_key.read().await.is_some();

        let session_text = match self.riot_client.lcu_get(CHAMP_SELECT_SESSION).await {
            Ok(text) => text,
            Err(e) => {
                let mut snapshot = self.empty_snapshot(api_configured);
                snapshot.note = Some(format!(
                    "Нет связи с клиентом League: {}",
                    friendly_lcu_error(&e)
                ));
                return Ok(snapshot);
            }
        };

        let session: Value = match serde_json::from_str(&session_text) {
            Ok(value) => value,
            Err(_) => {
                let mut snapshot = self.empty_snapshot(api_configured);
                snapshot.note =
                    Some("Клиент вернул неожиданный ответ на запрос выбора чемпионов.".to_string());
                return Ok(snapshot);
            }
        };

        let my_team = session
            .get("myTeam")
            .and_then(|team| team.as_array())
            .cloned()
            .unwrap_or_default();

        if my_team.is_empty() {
            let mut snapshot = self.empty_snapshot(api_configured);
            snapshot.note = Some(if is_lcu_error_body(&session) {
                "Войди в выбор чемпионов — данные подтянутся автоматически.".to_string()
            } else {
                "Выбор чемпионов пуст: вы ещё не в игре.".to_string()
            });
            return Ok(snapshot);
        }

        let local_cell = json_i32(session.get("localPlayerCellId"));
        self.log_session_shape(&session, &my_team).await;

        let (queue_id, mut is_custom) = self.gameflow_queue().await;
        if !is_custom {
            is_custom = session
                .get("isCustomGame")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
        }
        let is_ranked = matches!(
            queue_id,
            QUEUE_RANKED_SOLO | QUEUE_RANKED_FLEX | QUEUE_RANKED_SOLO_OLD
        );

        // Все источники, по которым клиент может знать ники союзников.
        let known = self.known_identities(&session).await;
        let mut api_error: Option<String> = None;

        let mut allies: Vec<PlayerInfo> = Vec::new();
        for member in &my_team {
            allies.push(
                self.build_player(
                    member,
                    1,
                    local_cell,
                    &known,
                    api_configured,
                    &mut api_error,
                )
                .await,
            );
        }
        allies.sort_by_key(|player| player.cell_id);

        let their_team = session
            .get("theirTeam")
            .and_then(|team| team.as_array())
            .cloned()
            .unwrap_or_default();
        let mut enemies: Vec<PlayerInfo> = Vec::new();
        for member in &their_team {
            enemies.push(
                self.build_player(
                    member,
                    2,
                    local_cell,
                    &known,
                    api_configured,
                    &mut api_error,
                )
                .await,
            );
        }
        enemies.sort_by_key(|player| player.cell_id);

        let hidden_allies = allies.iter().filter(|p| p.identity_hidden).count();
        let named_allies = allies.len().saturating_sub(hidden_allies);
        let unmatched_names = unmatched_identities(&known, &allies);
        let note = build_note(is_ranked, hidden_allies, enemies.len(), is_custom);

        let stats = SourceStats {
            slots: my_team.len() + their_team.len(),
            slots_with_ids: my_team
                .iter()
                .chain(their_team.iter())
                .filter(|member| parse_slot(member, 0).has_any_id())
                .count(),
            known_identities: known.len(),
        };
        self.log_summary(
            &stats,
            &format!(
                "queue={} allies={} named={} hidden={} enemies={} unmatched={}",
                queue_id,
                allies.len(),
                named_allies,
                hidden_allies,
                enemies.len(),
                unmatched_names.len()
            ),
        )
        .await;

        Ok(RevealSnapshot {
            allies,
            enemies,
            in_champ_select: true,
            queue_id,
            queue_name: queue_name(queue_id).to_string(),
            is_ranked,
            is_custom,
            identity_hidden: hidden_allies > 0,
            api_configured,
            unmatched_names,
            api_error,
            note,
        })
    }

    fn empty_snapshot(&self, api_configured: bool) -> RevealSnapshot {
        RevealSnapshot {
            api_configured,
            ..Default::default()
        }
    }

    async fn gameflow_queue(&self) -> (i64, bool) {
        let Ok(text) = self.riot_client.lcu_get(GAMEFLOW_SESSION).await else {
            return (0, false);
        };
        let Ok(value) = serde_json::from_str::<Value>(&text) else {
            return (0, false);
        };
        let queue_id = value
            .pointer("/gameData/queue/id")
            .and_then(|id| id.as_i64())
            .unwrap_or(0);
        let is_custom = value
            .pointer("/gameData/isCustomGame")
            .and_then(|custom| custom.as_bool())
            .unwrap_or(false);
        (queue_id, is_custom)
    }

    async fn current_summoner(&self) -> Option<Identity> {
        self.identity_at(CURRENT_SUMMONER).await
    }

    async fn identity_at(&self, endpoint: &str) -> Option<Identity> {
        let text = self.riot_client.lcu_get(endpoint).await.ok()?;
        let value: Value = serde_json::from_str(&text).ok()?;
        identity_from_json(&value)
    }

    async fn summoner_by_id(&self, summoner_id: i64) -> Option<Identity> {
        let endpoints = [
            format!("/lol-summoner/v1/summoners/{}", summoner_id),
            format!("/lol-summoner/v2/summoners/{}", summoner_id),
        ];
        for endpoint in endpoints {
            if let Some(identity) = self.identity_at(&endpoint).await {
                return Some(identity);
            }
        }
        None
    }

    /// Главный трюк разведки: Riot скрывает ник в интерфейсе, но LCU отдаёт
    /// PUUID союзника, а официальный account-v1 по PUUID возвращает Riot ID.
    async fn account_name_by_puuid(&self, puuid: &str) -> Option<(String, String)> {
        if puuid.len() < MIN_PUUID_LEN {
            return None;
        }
        if let Some(cached) = self.cached_name(puuid).await {
            return cached;
        }

        let region = self.region.read().await.clone();
        let host = regional_host_for_platform(&region);
        let url = format!(
            "https://{}.api.riotgames.com/riot/account/v1/accounts/by-puuid/{}",
            host, puuid
        );

        match self.riot_api_get(&url).await {
            Ok(body) => {
                let name = serde_json::from_str::<Value>(&body)
                    .ok()
                    .and_then(|value| account_name_from_json(&value));
                // Отрицательный результат кешируем: аккаунт мог быть переименован.
                self.store_name(puuid, name.clone()).await;
                name
            }
            Err(e) => {
                // 404 — аккаунта нет, кешируем; 401/403/429 — ключ, повторим позже.
                if e.to_string().contains("404") {
                    self.store_name(puuid, None).await;
                }
                None
            }
        }
    }

    async fn cached_name(&self, puuid: &str) -> Option<Option<(String, String)>> {
        let cache = self.name_cache.lock().await;
        match cache.get(puuid) {
            Some((stored_at, name)) if stored_at.elapsed() < NAME_CACHE_TTL => Some(name.clone()),
            _ => None,
        }
    }

    async fn store_name(&self, puuid: &str, name: Option<(String, String)>) {
        let mut cache = self.name_cache.lock().await;
        if cache.len() >= NAME_CACHE_MAX {
            cache.retain(|_, (stored_at, _)| stored_at.elapsed() < NAME_CACHE_TTL);
            if cache.len() >= NAME_CACHE_MAX {
                cache.clear();
            }
        }
        cache.insert(puuid.to_string(), (Instant::now(), name));
    }

    async fn summoner_by_puuid(&self, puuid: &str) -> Option<Identity> {
        let encoded = urlencoding::encode(puuid);
        let endpoints = [
            format!("/lol-summoner/v1/summoners/by-puuid/{}", encoded),
            format!("/lol-summoner/v2/summoners/by-puuid/{}", encoded),
        ];
        for endpoint in endpoints {
            if let Some(identity) = self.identity_at(&endpoint).await {
                return Some(identity);
            }
        }
        None
    }

    /// Собирает всех, кого клиент знает поимённо: участники лобби и чата
    /// выбора чемпионов. Именно чат остаётся последним местом, где клиент
    /// хранит настоящие Riot ID, даже когда в интерфейсе показывает «Ally 1».
    async fn known_identities(&self, session: &Value) -> HashMap<String, Identity> {
        let mut map: HashMap<String, Identity> = HashMap::new();

        if let Ok(text) = self.riot_client.lcu_get(LOBBY_MEMBERS).await {
            if let Ok(members) = serde_json::from_str::<Vec<Value>>(&text) {
                for member in members {
                    if let Some(identity) = identity_from_json(&member) {
                        insert_identity(&mut map, identity);
                    }
                }
            }
        }

        for (key, identity) in self.champ_select_chat_identities(session).await {
            map.entry(key).or_insert(identity);
        }

        // Свой аккаунт: иногда клиент не отдаёт даже собственную строку.
        if let Some(me) = self.current_summoner().await {
            insert_identity(&mut map, me);
        }

        map
    }

    async fn champ_select_chat_identities(&self, session: &Value) -> HashMap<String, Identity> {
        let mut map: HashMap<String, Identity> = HashMap::new();
        let mut rooms: Vec<String> = Vec::new();

        let room_id = json_str(session.pointer("/chatDetails/chatRoomName"));
        if !room_id.is_empty() {
            rooms.push(room_id);
        }

        if let Ok(text) = self.riot_client.lcu_get(CHAT_CONVERSATIONS).await {
            if let Ok(conversations) = serde_json::from_str::<Vec<Value>>(&text) {
                for conversation in conversations {
                    let id = json_str(conversation.get("id"));
                    let kind = json_str(conversation.get("type"));
                    let is_champ_select =
                        kind.eq_ignore_ascii_case("champSelect") || id.contains("champ-select");
                    if !id.is_empty() && is_champ_select && !rooms.contains(&id) {
                        rooms.push(id);
                    }
                }
            }
        }

        // Шорткат клиента для чата выбора чемпионов.
        if !rooms.iter().any(|room| room == "champ-select") {
            rooms.push("champ-select".to_string());
        }

        let mut seen_participants: Vec<Value> = Vec::new();
        for room in &rooms {
            let encoded = urlencoding::encode(room);
            let endpoint = format!("/lol-chat/v1/conversations/{}/participants", encoded);
            let Ok(text) = self.riot_client.lcu_get(&endpoint).await else {
                continue;
            };
            let Ok(participants) = serde_json::from_str::<Vec<Value>>(&text) else {
                continue;
            };
            for participant in participants {
                if let Some(identity) = identity_from_json(&participant) {
                    insert_identity(&mut map, identity);
                }
                seen_participants.push(participant);
            }
        }

        self.log_chat_shape(rooms.len(), &seen_participants).await;

        map
    }

    /// Диагностика чата: есть ли в участниках настоящие Riot ID, или клиент
    /// отдал только заглушки «Ally N». Пишем в лог один раз на изменение.
    async fn log_chat_shape(&self, rooms: usize, participants: &[Value]) {
        let line = format!(
            "chat rooms={} participants={} fields=[{}]",
            rooms,
            participants.len(),
            field_names(participants)
        );
        let mut last = self.last_chat_log.lock().await;
        if last.as_deref() == Some(line.as_str()) {
            return;
        }
        *last = Some(line.clone());
        drop(last);
        self.logger.info(&format!("[Reveal] {}", line));
    }

    async fn build_player(
        &self,
        member: &Value,
        fallback_team: i32,
        local_cell: i32,
        known: &HashMap<String, Identity>,
        api_configured: bool,
        api_error: &mut Option<String>,
    ) -> PlayerInfo {
        let slot = parse_slot(member, fallback_team);
        let is_self = local_cell >= 0 && slot.cell_id == local_cell;

        let mut identity = Identity {
            game_name: slot.game_name.clone(),
            tag_line: slot.tag_line.clone(),
            puuid: slot.puuid.clone(),
            summoner_id: slot.summoner_id,
            ..Default::default()
        };

        if let Some(found) = lookup_known(&slot, known) {
            merge_identity(&mut identity, found);
        }

        // Уровень и иконку клиент отдаёт только по id/puuid.
        if identity.game_name.is_empty() && !identity.puuid.is_empty() {
            if let Some(summoner) = self.summoner_by_puuid(&identity.puuid).await {
                merge_identity(&mut identity, &summoner);
            }
        }
        if identity.summoner_id != 0
            && (identity.level == 0
                || identity.profile_icon_id == 0
                || identity.game_name.is_empty())
        {
            if let Some(summoner) = self.summoner_by_id(identity.summoner_id).await {
                merge_identity(&mut identity, &summoner);
            }
        }

        // Последний и самый надёжный источник: PUUID → Riot ID через Riot API.
        // Работает, даже если клиент показал «Ally 1» и не отдал gameName.
        if identity.game_name.is_empty() && api_configured && identity.puuid.len() >= MIN_PUUID_LEN
        {
            if let Some((game_name, tag_line)) = self.account_name_by_puuid(&identity.puuid).await {
                if identity.game_name.is_empty() {
                    identity.game_name = game_name;
                }
                if identity.tag_line.is_empty() {
                    identity.tag_line = tag_line;
                }
            }
        }

        let mut tier = "Unranked".to_string();
        let mut rank = String::new();
        let mut league_points = 0;
        let mut wins = 0;
        let mut losses = 0;

        if api_configured && identity.puuid.len() >= MIN_PUUID_LEN {
            match self.ranked_for(&identity.puuid).await {
                Ok(Some(ranked)) => {
                    if !ranked.tier.is_empty() {
                        tier = ranked.tier;
                    }
                    rank = ranked.rank;
                    league_points = ranked.league_points;
                    wins = ranked.wins;
                    losses = ranked.losses;
                }
                Ok(None) => {}
                Err(e) => {
                    if api_error.is_none() {
                        *api_error = Some(e.to_string());
                    }
                }
            }
        }

        let riot_id = riot_id_of(&identity);

        PlayerInfo {
            riot_id,
            summoner_name: identity.game_name.clone(),
            champion_id: slot.champion_id,
            rank,
            tier,
            league_points,
            wins,
            losses,
            win_rate: win_rate(wins, losses),
            level: identity.level,
            profile_icon_id: identity.profile_icon_id,
            puuid: identity.puuid,
            ugg_link: String::new(),
            cell_id: slot.cell_id,
            team: slot.team,
            position: slot.position,
            is_self,
            identity_hidden: identity.game_name.is_empty(),
        }
    }

    // --- Ранги ---

    async fn cached_ranked(&self, puuid: &str) -> Option<RankedInfo> {
        let cache = self.ranked_cache.lock().await;
        match cache.get(puuid) {
            Some((stored_at, info)) if stored_at.elapsed() < RANKED_CACHE_TTL => Some(info.clone()),
            _ => None,
        }
    }

    async fn store_ranked(&self, puuid: &str, info: &RankedInfo) {
        let mut cache = self.ranked_cache.lock().await;
        if cache.len() >= RANKED_CACHE_MAX {
            cache.retain(|_, (stored_at, _)| stored_at.elapsed() < RANKED_CACHE_TTL);
            if cache.len() >= RANKED_CACHE_MAX {
                cache.clear();
            }
        }
        cache.insert(puuid.to_string(), (Instant::now(), info.clone()));
    }

    async fn ranked_for(&self, puuid: &str) -> Result<Option<RankedInfo>, AppError> {
        if puuid.len() < MIN_PUUID_LEN {
            return Ok(None);
        }
        if let Some(cached) = self.cached_ranked(puuid).await {
            return Ok(Some(cached));
        }

        let region = self.region.read().await.clone();
        let url = format!(
            "https://{}.api.riotgames.com/lol/league/v4/entries/by-puuid/{}",
            region, puuid
        );
        let body = self.riot_api_get(&url).await?;
        let entries: Vec<Value> = serde_json::from_str(&body).unwrap_or_default();

        let mut info = RankedInfo::default();
        for entry in entries {
            let queue = entry
                .get("queueType")
                .and_then(|q| q.as_str())
                .unwrap_or("");
            if queue == "RANKED_SOLO_5x5" {
                info.tier = entry
                    .get("tier")
                    .and_then(|t| t.as_str())
                    .unwrap_or("")
                    .to_string();
                info.rank = entry
                    .get("rank")
                    .and_then(|r| r.as_str())
                    .unwrap_or("")
                    .to_string();
                info.league_points = json_i32(entry.get("leaguePoints"));
                info.wins = json_i32(entry.get("wins"));
                info.losses = json_i32(entry.get("losses"));
                break;
            }
        }

        self.store_ranked(puuid, &info).await;
        Ok(Some(info))
    }

    // --- Последние матчи аккаунта ---

    pub async fn get_recent_matches(
        &self,
        puuid: &str,
        server: &str,
        count: u32,
    ) -> Result<Vec<MatchSummary>, AppError> {
        if !crate::models::account::is_valid_puuid(puuid) {
            return Err(AppError::Custom(
                "У аккаунта нет корректного PUUID — обнови профиль «Из клиента» и попробуй снова."
                    .to_string(),
            ));
        }

        let count = count.clamp(1, 10);
        let cache_key = format!("{}:{}:{}", puuid, server, count);
        if let Some(cached) = self.cached_matches(&cache_key).await {
            return Ok(cached);
        }

        let regional = regional_host_for_server(server);
        let ids_url = format!(
            "https://{}.api.riotgames.com/lol/match/v5/matches/by-puuid/{}/ids?start=0&count={}",
            regional, puuid, count
        );
        let ids_body = self.riot_api_get(&ids_url).await?;
        let match_ids: Vec<String> = serde_json::from_str(&ids_body).unwrap_or_default();

        let mut matches: Vec<MatchSummary> = Vec::new();
        for match_id in match_ids {
            let url = format!(
                "https://{}.api.riotgames.com/lol/match/v5/matches/{}",
                regional, match_id
            );
            match self.riot_api_get(&url).await {
                Ok(body) => {
                    if let Ok(value) = serde_json::from_str::<Value>(&body) {
                        if let Some(summary) = parse_match_summary(&value, puuid) {
                            matches.push(summary);
                        }
                    }
                }
                Err(e) => {
                    // Уже загруженные матчи не выбрасываем: показать 2 из 5 лучше,
                    // чем ничего, но ошибку наверх отдаём.
                    if matches.is_empty() {
                        return Err(e);
                    }
                    break;
                }
            }
        }

        self.store_matches(&cache_key, &matches).await;
        Ok(matches)
    }

    async fn cached_matches(&self, key: &str) -> Option<Vec<MatchSummary>> {
        let cache = self.match_cache.lock().await;
        match cache.get(key) {
            Some((stored_at, matches)) if stored_at.elapsed() < MATCH_CACHE_TTL => {
                Some(matches.clone())
            }
            _ => None,
        }
    }

    async fn store_matches(&self, key: &str, matches: &[MatchSummary]) {
        let mut cache = self.match_cache.lock().await;
        if cache.len() >= MATCH_CACHE_MAX {
            cache.retain(|_, (stored_at, _)| stored_at.elapsed() < MATCH_CACHE_TTL);
            if cache.len() >= MATCH_CACHE_MAX {
                cache.clear();
            }
        }
        cache.insert(key.to_string(), (Instant::now(), matches.to_vec()));
    }

    // --- Диагностика ---

    /// Пишем в лог, какие поля клиент реально отдал в сессии чемпионского
    /// выбора: это единственный способ увидеть анонимизацию Riot без отладчика.
    async fn log_session_shape(&self, session: &Value, my_team: &[Value]) {
        let game_id = json_i64(session.get("gameId"));
        {
            let mut last = self.last_session_log.lock().await;
            if let Some((logged_game, logged_at)) = *last {
                let same_game = logged_game == game_id;
                let recently = logged_at.elapsed() < Duration::from_secs(60);
                if same_game && (game_id != 0 || recently) {
                    return;
                }
            }
            *last = Some((game_id, Instant::now()));
        }

        let their_team: Vec<Value> = session
            .get("theirTeam")
            .and_then(|team| team.as_array())
            .cloned()
            .unwrap_or_default();

        self.logger.info(&format!(
            "[Reveal] session gameId={} localCell={} myTeam={} theirTeam={} myTeamKeys=[{}] theirTeamKeys=[{}] chatRoom={}",
            game_id,
            json_i32(session.get("localPlayerCellId")),
            my_team.len(),
            their_team.len(),
            field_names(my_team),
            field_names(&their_team),
            json_str(session.pointer("/chatDetails/chatRoomName"))
        ));
    }

    async fn log_summary(&self, stats: &SourceStats, summary: &str) {
        let line = format!(
            "{} slots={} slots_with_ids={} known_identities={}",
            summary, stats.slots, stats.slots_with_ids, stats.known_identities
        );
        let mut last = self.last_summary_log.lock().await;
        if last.as_deref() == Some(line.as_str()) {
            return;
        }
        *last = Some(line.clone());
        drop(last);
        self.logger.info(&format!("[Reveal] {}", line));
    }

    pub async fn send_message_to_chat(&self, message: &str) -> Result<bool, AppError> {
        let body = serde_json::json!({
            "body": message,
            "type": "chat"
        })
        .to_string();

        let resp = self
            .riot_client
            .lcu_post("/lol-chat/v1/conversations/champ-select/messages", &body)
            .await?;

        Ok(!resp.contains("errorCode"))
    }
}

// --- Разбор данных LCU (чистые функции, покрыты тестами) ---

fn parse_slot(member: &Value, fallback_team: i32) -> Slot {
    let team = json_i32(member.get("team"));

    let mut puuid = json_str(member.get("puuid"));
    if puuid.is_empty() {
        puuid = json_str(member.get("encryptedPuuid"));
    }

    let mut game_name = json_str(member.get("gameName"));
    if game_name.is_empty() {
        game_name = json_str(member.get("summonerName"));
    }

    Slot {
        cell_id: json_i32(member.get("cellId")),
        team: if team == 0 { fallback_team } else { team },
        champion_id: json_i32(member.get("championId")),
        summoner_id: json_i64(member.get("summonerId")),
        puuid,
        game_name,
        tag_line: json_str(member.get("tagLine")),
        position: json_str(member.get("assignedPosition")),
    }
}

/// Достаёт идентичность из любого ответа LCU о призывателе: сессия чемпионского
/// выбора, `/lol-summoner/...`, участник лобби или чата.
fn identity_from_json(value: &Value) -> Option<Identity> {
    let mut game_name = json_str(value.get("gameName"));
    let mut tag_line = json_str(value.get("tagLine"));
    if tag_line.is_empty() {
        tag_line = json_str(value.get("gameTag"));
    }

    if game_name.is_empty() {
        let display_name = json_str(value.get("displayName"));
        if !display_name.is_empty() {
            match display_name.rsplit_once('#') {
                Some((name, tag)) => {
                    game_name = name.to_string();
                    tag_line = tag.to_string();
                }
                None => game_name = display_name,
            }
        }
    }
    if game_name.is_empty() {
        game_name = json_str(value.get("summonerName"));
    }

    // Заглушки клиента («Ally 1», «Союзник 2») ником не считаем, но id
    // сохраняем: по нему ник ещё можно поднять из другого источника.
    if looks_anonymized(&game_name) {
        game_name = String::new();
        tag_line = String::new();
    }

    let mut puuid = json_str(value.get("puuid"));
    if puuid.is_empty() {
        puuid = json_str(value.get("encryptedPuuid"));
    }

    let summoner_id = json_i64(value.get("summonerId"));

    if game_name.is_empty() && puuid.is_empty() && summoner_id == 0 {
        return None;
    }

    Some(Identity {
        game_name,
        tag_line,
        puuid,
        summoner_id,
        level: json_i32(value.get("summonerLevel")),
        profile_icon_id: json_i32(value.get("profileIconId")),
    })
}

/// Клиент отдаёт вместо ника заглушку вида «Ally 1» / «Союзник 2».
fn looks_anonymized(name: &str) -> bool {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return false;
    }
    let lower = trimmed.to_lowercase();
    lower.starts_with("ally ")
        || lower.starts_with("союзник ")
        || lower.starts_with("player ")
        || lower.starts_with("teammate ")
}

fn insert_identity(map: &mut HashMap<String, Identity>, identity: Identity) {
    if identity.summoner_id != 0 {
        map.insert(format!("sid:{}", identity.summoner_id), identity.clone());
    }
    if !identity.puuid.is_empty() {
        map.insert(format!("puuid:{}", identity.puuid), identity);
    }
}

fn lookup_known<'a>(slot: &Slot, known: &'a HashMap<String, Identity>) -> Option<&'a Identity> {
    if slot.summoner_id != 0 {
        if let Some(identity) = known.get(&format!("sid:{}", slot.summoner_id)) {
            return Some(identity);
        }
    }
    if !slot.puuid.is_empty() {
        if let Some(identity) = known.get(&format!("puuid:{}", slot.puuid)) {
            return Some(identity);
        }
    }
    None
}

/// Дополняет уже известное только отсутствующими полями: данные из сессии
/// чемпионского выбора приоритетнее лобби и справочника призывателей.
fn merge_identity(base: &mut Identity, extra: &Identity) {
    if base.game_name.is_empty() {
        base.game_name = extra.game_name.clone();
    }
    if base.tag_line.is_empty() {
        base.tag_line = extra.tag_line.clone();
    }
    if base.puuid.is_empty() {
        base.puuid = extra.puuid.clone();
    }
    if base.summoner_id == 0 {
        base.summoner_id = extra.summoner_id;
    }
    if base.level == 0 {
        base.level = extra.level;
    }
    if base.profile_icon_id == 0 {
        base.profile_icon_id = extra.profile_icon_id;
    }
}

fn riot_id_of(identity: &Identity) -> String {
    if identity.game_name.is_empty() {
        return String::new();
    }
    if identity.tag_line.is_empty() {
        identity.game_name.clone()
    } else {
        format!("{}#{}", identity.game_name, identity.tag_line)
    }
}

fn win_rate(wins: i32, losses: i32) -> String {
    if wins + losses <= 0 {
        "N/A".to_string()
    } else {
        format!("{:.0}%", (wins as f64 / (wins + losses) as f64) * 100.0)
    }
}

fn queue_name(queue_id: i64) -> String {
    match queue_id {
        QUEUE_RANKED_SOLO | QUEUE_RANKED_SOLO_OLD => "Ranked Solo/Duo".into(),
        QUEUE_RANKED_FLEX => "Ranked Flex".into(),
        400 => "Normal Draft".into(),
        430 => "Normal Blind".into(),
        450 => "ARAM".into(),
        480 => "Swiftplay".into(),
        490 => "Normal Quickplay".into(),
        700 => "Clash".into(),
        720 => "ARAM Clash".into(),
        830 | 840 | 850 | 870 | 880 | 890 => "Co-op vs AI".into(),
        900 | 1900 => "URF".into(),
        1010 => "Snow ARURF".into(),
        1020 => "One for All".into(),
        1200 | 1300 => "Nexus Blitz".into(),
        1400 => "Ultimate Spellbook".into(),
        1700 | 1710 => "Arena".into(),
        1810 | 1820 | 1830 => "Swarm".into(),
        2400 => "ARAM: Mayhem".into(),
        2000 | 2010 | 2020 => "Tutorial".into(),
        0 => String::new(),
        _ => format!("Режим #{queue_id}"),
    }
}

/// Платформа → регион для account-v1 и match-v5.
fn regional_host_for_platform(region: &str) -> &'static str {
    match region.trim().to_lowercase().as_str() {
        "euw1" | "eun1" | "tr1" | "ru" => "europe",
        "na1" | "br1" | "la1" | "la2" => "americas",
        "kr" | "jp1" => "asia",
        "oc1" | "ph2" | "sg2" | "th2" | "tw2" | "vn2" => "sea",
        _ => "europe",
    }
}

/// Платформа аккаунта → регион для match-v5.
fn regional_host_for_server(server: &str) -> &'static str {
    let upper = server.trim().to_uppercase();
    match upper.as_str() {
        "EUW" | "EUW1" | "EUNE" | "EUN1" | "RU" | "TR" | "TR1" => "europe",
        "NA" | "NA1" | "BR" | "BR1" | "LAN" | "LA1" | "LAS" | "LA2" => "americas",
        "KR" | "JP" | "JP1" => "asia",
        "OCE" | "OC1" => "sea",
        _ => "europe",
    }
}

/// Ники из известных источников, которые не удалось привязать ни к одному
/// слоту: бывает, когда клиент скрыл id, но чат ещё отдаёт имена.
fn unmatched_identities(known: &HashMap<String, Identity>, allies: &[PlayerInfo]) -> Vec<String> {
    let ally_names: Vec<String> = allies
        .iter()
        .map(|player| player.summoner_name.to_lowercase())
        .filter(|name| !name.is_empty())
        .collect();

    let mut names: Vec<String> = Vec::new();
    for identity in known.values() {
        let name = identity.game_name.trim();
        if name.is_empty() {
            continue;
        }
        if ally_names
            .iter()
            .any(|known_name| known_name == &name.to_lowercase())
        {
            continue;
        }
        let display = riot_id_of(identity);
        if !names.contains(&display) {
            names.push(display);
        }
    }
    names.sort();
    names.truncate(10);
    names
}

fn account_name_from_json(value: &Value) -> Option<(String, String)> {
    let game_name = json_str(value.get("gameName"));
    if game_name.is_empty() {
        return None;
    }
    Some((game_name, json_str(value.get("tagLine"))))
}

pub(crate) fn parse_match_summary(value: &Value, puuid: &str) -> Option<MatchSummary> {
    let info = value.get("info")?;
    let participants = info.get("participants")?.as_array()?;
    let me = participants
        .iter()
        .find(|participant| json_str(participant.get("puuid")) == puuid)?;

    Some(MatchSummary {
        match_id: json_str(value.pointer("/metadata/matchId")),
        champion_id: json_i32(me.get("championId")),
        champion_name: json_str(me.get("championName")),
        win: me.get("win").and_then(|win| win.as_bool()).unwrap_or(false),
        kills: json_i32(me.get("kills")),
        deaths: json_i32(me.get("deaths")),
        assists: json_i32(me.get("assists")),
        queue_name: queue_name(json_i64(info.get("queueId"))).to_string(),
        position: json_str(me.get("teamPosition")),
        duration_secs: json_i64(info.get("gameDuration")),
        created_at_ms: json_i64(info.get("gameCreation")),
    })
}

/// Объяснение для UI: почему список неполный. Пусто — всё на месте.
fn build_note(
    is_ranked: bool,
    hidden_allies: usize,
    enemies_total: usize,
    is_custom: bool,
) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();

    if hidden_allies > 0 {
        if is_ranked {
            parts.push(
                "Клиент не отдал ники части союзников. Riot анонимизирует ранговый выбор, но приложение дотягивает ники из чата и лобби — если тут пусто, значит закрыто и там. Полный разбор полей пишется в «Логи»."
                    .to_string(),
            );
        } else {
            parts.push(
                "Клиент не отдал ники части союзников — показываем то, что доступно.".to_string(),
            );
        }
    }

    if enemies_total == 0 && !is_custom {
        parts.push(
            "Противников Riot в выборе чемпионов не показывает — они видны только в кастомных лобби.".to_string(),
        );
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" "))
    }
}

fn friendly_lcu_error(error: &AppError) -> String {
    let text = error.to_string();
    if text.contains("LCU not connected") {
        "клиент League не запущен или ещё не авторизован.".to_string()
    } else {
        text
    }
}

/// LCU отвечает на 404 объектом с `httpStatus`/`errorCode` вместо сессии.
fn is_lcu_error_body(value: &Value) -> bool {
    value.get("httpStatus").is_some() || value.get("errorCode").is_some()
}

fn field_names(members: &[Value]) -> String {
    let mut names: Vec<String> = Vec::new();
    for member in members {
        if let Some(object) = member.as_object() {
            for key in object.keys() {
                if !names.iter().any(|existing| existing == key) {
                    names.push(key.clone());
                }
            }
        }
    }
    names.sort();
    names.join(",")
}

fn json_i64(value: Option<&Value>) -> i64 {
    match value {
        Some(Value::Number(number)) => number.as_i64().unwrap_or(0),
        Some(Value::String(text)) => text.trim().parse().unwrap_or(0),
        _ => 0,
    }
}

fn json_i32(value: Option<&Value>) -> i32 {
    json_i64(value) as i32
}

fn json_str(value: Option<&Value>) -> String {
    value
        .and_then(|inner| inner.as_str())
        .unwrap_or("")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_slot_reads_full_ranked_payload() {
        let member = json!({
            "cellId": 2,
            "team": 1,
            "championId": 103,
            "summonerId": 51581084,
            "puuid": "abcdefghijklmnopqrstuvwxyz0123456789abcdefghijklmnopqrstuvwxyz0123456789ab",
            "gameName": "Faker",
            "tagLine": "KR1",
            "assignedPosition": "middle",
            "playerType": "PLAYER"
        });

        let slot = parse_slot(&member, 1);
        assert_eq!(slot.cell_id, 2);
        assert_eq!(slot.team, 1);
        assert_eq!(slot.champion_id, 103);
        assert_eq!(slot.summoner_id, 51581084);
        assert_eq!(slot.game_name, "Faker");
        assert_eq!(slot.tag_line, "KR1");
        assert_eq!(slot.position, "middle");
        assert!(slot.has_any_id());
    }

    /// Ранговый выбор 2025+: Riot отдаёт только слот и чемпиона. Раньше такой
    /// слот отбрасывался (`return None`) — из-за этого список союзников пустел.
    #[test]
    fn parse_slot_keeps_anonymized_member() {
        let member = json!({
            "cellId": 3,
            "team": 1,
            "championId": 0,
            "summonerId": 0,
            "assignedPosition": "utility"
        });

        let slot = parse_slot(&member, 1);
        assert_eq!(slot.cell_id, 3);
        assert_eq!(slot.position, "utility");
        assert!(slot.game_name.is_empty());
        assert!(slot.puuid.is_empty());
        assert!(!slot.has_any_id());
    }

    #[test]
    fn parse_slot_accepts_summoner_id_as_string() {
        let member = json!({
            "cellId": 1,
            "summonerId": "51581084",
            "team": 2
        });

        let slot = parse_slot(&member, 2);
        assert_eq!(slot.summoner_id, 51581084);
        assert_eq!(slot.team, 2);
    }

    #[test]
    fn identity_from_summoner_payload() {
        let value = json!({
            "gameName": "Faker",
            "tagLine": "KR1",
            "puuid": "puuid-value",
            "summonerId": 42,
            "summonerLevel": 512,
            "profileIconId": 7
        });

        let identity = identity_from_json(&value).expect("identity");
        assert_eq!(identity.game_name, "Faker");
        assert_eq!(identity.tag_line, "KR1");
        assert_eq!(identity.puuid, "puuid-value");
        assert_eq!(identity.summoner_id, 42);
        assert_eq!(identity.level, 512);
        assert_eq!(identity.profile_icon_id, 7);
    }

    #[test]
    fn identity_from_lobby_display_name() {
        let value = json!({
            "displayName": "Faker#KR1",
            "summonerId": 42
        });

        let identity = identity_from_json(&value).expect("identity");
        assert_eq!(identity.game_name, "Faker");
        assert_eq!(identity.tag_line, "KR1");
    }

    /// Участник чата выбора чемпионов отдаёт Riot ID полями gameName/gameTag.
    #[test]
    fn identity_from_chat_participant() {
        let value = json!({
            "gameName": "Faker",
            "gameTag": "KR1",
            "puuid": "puuid-value",
            "summonerId": 42,
            "name": "Ally 1"
        });

        let identity = identity_from_json(&value).expect("identity");
        assert_eq!(identity.game_name, "Faker");
        assert_eq!(identity.tag_line, "KR1");
    }

    /// Заглушка «Ally 1» вместо ника: id сохраняем, ник — нет.
    #[test]
    fn anonymized_name_is_dropped_but_id_kept() {
        let value = json!({
            "gameName": "Ally 2",
            "puuid": "puuid-value",
            "summonerId": 42
        });

        let identity = identity_from_json(&value).expect("identity");
        assert!(identity.game_name.is_empty());
        assert_eq!(identity.puuid, "puuid-value");
        assert_eq!(identity.summoner_id, 42);
    }

    #[test]
    fn anonymized_detector() {
        assert!(looks_anonymized("Ally 1"));
        assert!(looks_anonymized("Союзник 3"));
        assert!(!looks_anonymized("Faker"));
        assert!(!looks_anonymized(""));
    }

    #[test]
    fn identity_from_lcu_error_body_is_none() {
        let value = json!({ "httpStatus": 404, "message": "Not found" });
        assert!(identity_from_json(&value).is_none());
    }

    #[test]
    fn known_lookup_matches_summoner_id_then_puuid() {
        let mut known: HashMap<String, Identity> = HashMap::new();
        insert_identity(
            &mut known,
            Identity {
                game_name: "Faker".to_string(),
                summoner_id: 42,
                ..Default::default()
            },
        );

        let by_id = Slot {
            summoner_id: 42,
            ..Default::default()
        };
        assert_eq!(
            lookup_known(&by_id, &known).map(|i| i.game_name.clone()),
            Some("Faker".to_string())
        );

        let by_puuid = Slot {
            puuid: "puuid-value".to_string(),
            ..Default::default()
        };
        assert!(lookup_known(&by_puuid, &known).is_none());
    }

    #[test]
    fn merge_identity_keeps_session_values() {
        let mut base = Identity {
            game_name: "Session".to_string(),
            puuid: String::new(),
            level: 0,
            ..Default::default()
        };
        let extra = Identity {
            game_name: "Lobby".to_string(),
            puuid: "puuid-value".to_string(),
            level: 512,
            ..Default::default()
        };

        merge_identity(&mut base, &extra);
        assert_eq!(base.game_name, "Session");
        assert_eq!(base.puuid, "puuid-value");
        assert_eq!(base.level, 512);
    }

    #[test]
    fn riot_id_formatting() {
        let full = Identity {
            game_name: "Faker".to_string(),
            tag_line: "KR1".to_string(),
            ..Default::default()
        };
        let without_tag = Identity {
            game_name: "Faker".to_string(),
            ..Default::default()
        };
        let empty = Identity::default();

        assert_eq!(riot_id_of(&full), "Faker#KR1");
        assert_eq!(riot_id_of(&without_tag), "Faker");
        assert_eq!(riot_id_of(&empty), "");
    }

    #[test]
    fn win_rate_handles_empty_history() {
        assert_eq!(win_rate(0, 0), "N/A");
        assert_eq!(win_rate(3, 1), "75%");
    }

    #[test]
    fn ranked_note_explains_hidden_allies() {
        let note = build_note(true, 5, 0, false).expect("note");
        assert!(note.contains("ники"));
        assert!(note.contains("Противников"));

        assert!(build_note(true, 0, 5, false).is_none());
    }

    #[test]
    fn custom_lobby_has_enemy_note_only_when_empty() {
        assert!(build_note(false, 0, 0, true).is_none());
        assert!(build_note(false, 0, 0, false).is_some());
    }

    #[test]
    fn queue_names() {
        assert_eq!(queue_name(420), "Ranked Solo/Duo");
        assert_eq!(queue_name(440), "Ranked Flex");
        assert_eq!(queue_name(480), "Swiftplay");
        assert_eq!(queue_name(1710), "Arena");
        assert_eq!(queue_name(9999), "Режим #9999");
        assert_eq!(queue_name(0), "");
    }

    #[test]
    fn unmatched_identities_skips_known_allies() {
        let mut known: HashMap<String, Identity> = HashMap::new();
        insert_identity(
            &mut known,
            Identity {
                game_name: "AllyKnown".to_string(),
                summoner_id: 1,
                ..Default::default()
            },
        );
        insert_identity(
            &mut known,
            Identity {
                game_name: "Mystery".to_string(),
                tag_line: "RU1".to_string(),
                summoner_id: 2,
                ..Default::default()
            },
        );

        let allies = vec![PlayerInfo {
            summoner_name: "AllyKnown".to_string(),
            ..Default::default()
        }];

        let unmatched = unmatched_identities(&known, &allies);
        assert_eq!(unmatched, vec!["Mystery#RU1".to_string()]);
    }

    #[test]
    fn account_name_from_riot_account_payload() {
        let value = json!({
            "puuid": "abcdefghijklmnopqrstuvwxyz0123456789abcdefghijklmnopqrstuvwxyz0123456789ab",
            "gameName": "Faker",
            "tagLine": "KR1"
        });
        let (game_name, tag_line) = account_name_from_json(&value).expect("name");
        assert_eq!(game_name, "Faker");
        assert_eq!(tag_line, "KR1");

        assert!(account_name_from_json(&json!({ "gameName": "" })).is_none());
        assert!(account_name_from_json(&json!({ "message": "Not Found" })).is_none());
    }

    #[test]
    fn regional_routing_by_platform_region() {
        assert_eq!(regional_host_for_platform("euw1"), "europe");
        assert_eq!(regional_host_for_platform("ru"), "europe");
        assert_eq!(regional_host_for_platform("na1"), "americas");
        assert_eq!(regional_host_for_platform("kr"), "asia");
        assert_eq!(regional_host_for_platform("oc1"), "sea");
        assert_eq!(regional_host_for_platform("unknown"), "europe");
    }

    #[test]
    fn regional_routing_by_server() {
        assert_eq!(regional_host_for_server("EUW"), "europe");
        assert_eq!(regional_host_for_server("RU"), "europe");
        assert_eq!(regional_host_for_server("NA"), "americas");
        assert_eq!(regional_host_for_server("LAN"), "americas");
        assert_eq!(regional_host_for_server("KR"), "asia");
        assert_eq!(regional_host_for_server("OCE"), "sea");
        assert_eq!(regional_host_for_server(""), "europe");
    }

    #[test]
    fn parse_match_summary_finds_my_participant() {
        let value = json!({
            "metadata": { "matchId": "EUW1_123" },
            "info": {
                "queueId": 420,
                "gameCreation": 1_760_000_000_000_i64,
                "gameDuration": 1980,
                "participants": [
                    { "puuid": "other", "championId": 1, "championName": "Annie", "win": false, "kills": 0, "deaths": 5, "assists": 1, "teamPosition": "TOP" },
                    { "puuid": "puuid-value", "championId": 103, "championName": "Ahri", "win": true, "kills": 7, "deaths": 2, "assists": 9, "teamPosition": "MIDDLE" }
                ]
            }
        });

        let summary = parse_match_summary(&value, "puuid-value").expect("summary");
        assert_eq!(summary.match_id, "EUW1_123");
        assert_eq!(summary.champion_id, 103);
        assert_eq!(summary.champion_name, "Ahri");
        assert!(summary.win);
        assert_eq!(summary.kills, 7);
        assert_eq!(summary.deaths, 2);
        assert_eq!(summary.assists, 9);
        assert_eq!(summary.queue_name, "Ranked Solo/Duo");
        assert_eq!(summary.position, "MIDDLE");
        assert_eq!(summary.duration_secs, 1980);
    }

    #[test]
    fn parse_match_summary_without_my_participant_is_none() {
        let value = json!({
            "metadata": { "matchId": "EUW1_123" },
            "info": { "participants": [] }
        });
        assert!(parse_match_summary(&value, "puuid-value").is_none());
    }
}
