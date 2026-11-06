use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

pub const SETTING: &str = "DraftCoordinationEnabled";
pub const OPT_IN: &str = "DraftCoordinationOptIn";

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PreferenceState {
    Must,
    Yield,
    Exclude,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChampionPreference {
    pub champion_id: i32,
    pub state: PreferenceState,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct DraftPreferences {
    pub pick: Vec<ChampionPreference>,
    pub ban: Vec<ChampionPreference>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PickContext {
    pub puuid: String,
    pub summoner_id: String,
    pub pick_order: Option<usize>,
    pub pick_active: bool,
    pub pick_role: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DraftIntent {
    pub session_key: String,
    pub mode: String,
    pub candidates: Vec<i32>,
    pub ban_candidates: Vec<i32>,
    pub preferences: DraftPreferences,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pick_order: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pick_active: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pick_role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pick_context: Option<Vec<PickContext>>,
}

/// LCU action groups define turn order; team row order never does.
pub fn classic_pick_turn(session: &Value) -> Option<(usize, bool)> {
    let cell = session["localPlayerCellId"]
        .as_i64()
        .filter(|id| *id >= 0)?;
    let groups = session["actions"].as_array()?;
    if groups.len() > 100 {
        return None;
    }
    let mut own = None;
    let banning = groups
        .iter()
        .filter_map(Value::as_array)
        .flatten()
        .any(|a| a["type"] == "ban" && a["isInProgress"] == true && a["completed"] != true);
    for (order, group) in groups.iter().enumerate() {
        for action in group.as_array()? {
            if action["type"] == "pick"
                && action["actorCellId"].as_i64() == Some(cell)
                && action["completed"] != true
            {
                if own.is_some() {
                    return None;
                }
                own = Some((order, !banning && action["isInProgress"] == true));
            }
        }
    }
    own
}

pub fn classic_pick_context(session: &Value) -> Option<Vec<PickContext>> {
    let team = session["myTeam"].as_array()?;
    if team.len() > 5 {
        return None;
    }
    let mut context = Vec::new();
    for player in team {
        let cell = player["cellId"].as_i64().filter(|id| *id >= 0)?;
        let mut view = session.clone();
        view["localPlayerCellId"] = serde_json::json!(cell);
        let turn = classic_pick_turn(&view);
        context.push(PickContext {
            puuid: player["puuid"].as_str().unwrap_or("").into(),
            summoner_id: positive_id(&player["summonerId"])
                .map(|id| id.to_string())
                .unwrap_or_default(),
            pick_order: turn.map(|t| t.0),
            pick_active: turn.is_some_and(|t| t.1),
            pick_role: super::champ_select_policy::role(player).into(),
        });
    }
    context.sort_by(|a, b| (&a.puuid, &a.summoner_id).cmp(&(&b.puuid, &b.summoner_id)));
    Some(context)
}

fn context_matches(member: &Value, context: &[PickContext]) -> bool {
    context.iter().any(|entry| {
        let identity = if crate::models::account::is_valid_puuid(&entry.puuid) {
            member["gamePuuid"].as_str() == Some(entry.puuid.as_str())
        } else {
            positive_id(&member["gameSummonerId"])
                .is_some_and(|id| entry.summoner_id == id.to_string())
        };
        identity
            && member["draft"]["pickOrder"].as_u64() == entry.pick_order.map(|n| n as u64)
            && member["draft"]["pickActive"].as_bool() == Some(entry.pick_active)
            && member["draft"]["pickRole"].as_str() == Some(entry.pick_role.as_str())
    })
}

/// Use the caller's fresh LCU context, never another peer's cached action order.
pub fn coordinate_for_actor(
    members: &Value,
    session_key: &str,
    mode: &str,
    actor: &str,
) -> BTreeMap<String, i32> {
    if mode != "classic" {
        return coordinate(members, session_key, mode);
    }
    let Some(users) = members.as_array() else {
        return BTreeMap::new();
    };
    let Some(own) = users.iter().find(|m| m["id"].as_str() == Some(actor)) else {
        return BTreeMap::new();
    };
    let Ok(context) =
        serde_json::from_value::<Vec<PickContext>>(own["draft"]["pickContext"].clone())
    else {
        return BTreeMap::new();
    };
    if context.len() > 5 {
        return BTreeMap::new();
    }
    let current: Vec<_> = users
        .iter()
        .filter(|m| context_matches(m, &context))
        .cloned()
        .collect();
    coordinate(&serde_json::json!(current), session_key, mode)
}

pub fn classic_occupied(session: &Value, id: i32) -> bool {
    ["myTeam", "theirTeam"].iter().any(|key| {
        session[*key].as_array().is_some_and(|team| {
            team.iter().any(|p| {
                p["cellId"] != session["localPlayerCellId"]
                    && p["championId"].as_i64() == Some(id as i64)
            })
        })
    }) || session["actions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_array)
        .flatten()
        .any(|a| {
            a["completed"] == true
                && matches!(a["type"].as_str(), Some("pick" | "ban"))
                && a["championId"].as_i64() == Some(id as i64)
        })
}

pub fn allowed_candidates(pool: &[ChampionPreference], available: &[i32]) -> Vec<i32> {
    let strict = pool
        .iter()
        .any(|p| p.state == PreferenceState::Must && available.contains(&p.champion_id));
    if pool.len() > 30
        || pool.iter().any(|p| !(1..=10000).contains(&p.champion_id))
        || pool
            .iter()
            .map(|p| p.champion_id)
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != pool.len()
    {
        return vec![];
    }
    pool.iter()
        .filter(|p| {
            (if strict {
                p.state == PreferenceState::Must
            } else {
                p.state == PreferenceState::Yield
            }) && available.contains(&p.champion_id)
        })
        .map(|p| p.champion_id)
        .collect()
}

/// Ban priorities are ordered only; saved traffic-light states are legacy data.
pub fn allowed_ban_candidates(pool: &[ChampionPreference], available: &[i32]) -> Vec<i32> {
    let neutral: Vec<_> = pool
        .iter()
        .map(|p| ChampionPreference {
            champion_id: p.champion_id,
            state: PreferenceState::Yield,
        })
        .collect();
    allowed_candidates(&neutral, available)
}

/// Deterministic planner only; callers validate the real group and reserve via server CAS.
pub fn coordinate(members: &Value, session_key: &str, mode: &str) -> BTreeMap<String, i32> {
    let empty = BTreeMap::new();
    let Some(members) = members.as_array() else {
        return empty;
    };
    if session_key.is_empty()
        || !session_key.starts_with(|c: char| c.is_ascii_digit() && c != '0')
        || !session_key.bytes().all(|c| c.is_ascii_digit())
        || members.len() > 5
    {
        return empty;
    }
    let mut seen = BTreeSet::new();
    for member in members {
        let Some(id) = member["id"].as_str().filter(|id| !id.is_empty()) else {
            return empty;
        };
        if !seen.insert(id) {
            return empty;
        }
    }
    let mut users: Vec<_> = members
        .iter()
        .filter(|m| {
            m["accepted"] == true
                && m["online"] == true
                && m["draft"]["sessionKey"] == session_key
                && m["draft"]["mode"] == mode
                && (mode != "classic"
                    || (m["draft"]["pickActive"] == true
                        && m["draft"]["pickOrder"].as_u64().is_some_and(|n| n <= 99)))
        })
        .collect();
    if mode == "classic" {
        let order = users
            .iter()
            .filter_map(|m| m["draft"]["pickOrder"].as_u64())
            .min();
        users.retain(|m| m["draft"]["pickOrder"].as_u64() == order);
    }
    users.sort_by_key(|m| m["id"].as_str().unwrap());
    let mut candidates = Vec::new();
    let mut green = Vec::new();
    let mut reservations = Vec::new();
    let mut reserved = BTreeSet::new();
    for user in &users {
        let draft = &user["draft"];
        let pool =
            serde_json::from_value::<Vec<ChampionPreference>>(draft["preferences"]["pick"].clone());
        let available = serde_json::from_value::<Vec<i32>>(draft["candidates"].clone());
        let allowed = match (&pool, &available) {
            (Ok(pool), Ok(ids))
                if ids.len() <= 30 && ids.iter().all(|id| (1..=10000).contains(id)) =>
            {
                allowed_candidates(pool, ids)
            }
            _ => vec![],
        };
        green.push(
            pool.as_ref()
                .map(|ps| {
                    ps.iter()
                        .filter(|p| p.state == PreferenceState::Must)
                        .map(|p| p.champion_id)
                        .collect::<BTreeSet<_>>()
                })
                .unwrap_or_default(),
        );
        let reservation = if user["draftChoice"].is_null() {
            None
        } else {
            let Some(id) = user["draftChoice"]
                .as_i64()
                .filter(|id| (1..=10000).contains(id))
            else {
                return empty;
            };
            let id = id as i32;
            if !allowed.contains(&id) || !reserved.insert(id) {
                return empty;
            }
            Some(id)
        };
        candidates.push(allowed);
        reservations.push(reservation);
    }
    // Preserve insertion order on equal scores, matching the TS Map traversal.
    let mut plans = vec![(0usize, (0i32, 0i32, 0i32, 0i32), BTreeMap::new())];
    let ids: BTreeSet<_> = candidates.iter().flatten().copied().collect();
    for id in ids {
        let mut next = plans.clone();
        for (mask, score, picks) in &plans {
            for (i, user) in users.iter().enumerate() {
                let Some(rank) = candidates[i].iter().position(|c| *c == id) else {
                    continue;
                };
                if mask & (1 << i) != 0
                    || reservations
                        .iter()
                        .position(|r| *r == Some(id))
                        .is_some_and(|owner| owner != i)
                    || reservations[i].is_some_and(|r| r != id)
                {
                    continue;
                }
                let key = mask | (1 << i);
                // Only the authoritative server ledger supplies turns; never infer a win from a reservation.
                let priority = user["greenPriority"]
                    .as_array()
                    .filter(|ps| ps.len() <= 120);
                let fairness = if mode != "classic" && green[i].contains(&id) {
                    users
                        .iter()
                        .enumerate()
                        .filter(|(j, peer)| {
                            *j != i
                                && green[*j].contains(&id)
                                && candidates[*j].contains(&id)
                                && priority.is_some_and(|ps| {
                                    ps.iter().any(|p| {
                                        p["championId"].as_i64() == Some(id as i64)
                                            && p["againstId"] == peer["id"]
                                    })
                                })
                        })
                        .count() as i32
                } else {
                    0
                };
                let score = (
                    score.0 + 1,
                    score.1 + i32::from(green[i].contains(&id)),
                    score.2 + fairness,
                    score.3 + 30 - rank as i32,
                );
                let mut picks = picks.clone();
                picks.insert(user["id"].as_str().unwrap().to_owned(), id);
                if let Some(existing) = next.iter_mut().find(|p| p.0 == key) {
                    if existing.1 < score {
                        *existing = (key, score, picks);
                    }
                } else {
                    next.push((key, score, picks));
                }
            }
        }
        plans = next;
    }
    plans
        .into_iter()
        .reduce(|a, b| if b.1 > a.1 { b } else { a })
        .unwrap()
        .2
}
pub fn local_preference_ids(
    mode: &str,
    role: &str,
    pool: &[i32],
    ban: bool,
    available: &[i32],
) -> Vec<i32> {
    let settings = crate::services::settings::SettingsService::new();
    let saved: BTreeMap<String, DraftPreferences> =
        settings.load_setting("DraftCoordinationPreferences", BTreeMap::new());
    let key = if mode == "classic" {
        format!("classic:{role}")
    } else {
        mode.into()
    };
    let overrides = saved.get(&key).filter(|_| !ban).map(|p| &p.pick);
    let preferences: Vec<ChampionPreference> = pool
        .iter()
        .copied()
        .filter(|id| (1..=10000).contains(id))
        .map(|id| ChampionPreference {
            champion_id: id,
            state: overrides
                .and_then(|ps| ps.iter().find(|p| p.champion_id == id))
                .map(|p| p.state)
                .unwrap_or(PreferenceState::Yield),
        })
        .collect();
    if ban {
        allowed_ban_candidates(&preferences, available)
    } else {
        allowed_candidates(&preferences, available)
    }
}

#[derive(Default)]
struct Cache {
    snapshot: Option<(Value, Instant, Duration)>,
    confirmation: Option<Value>,
}
impl Cache {
    fn invalidate(&mut self) {
        self.snapshot = None;
        self.confirmation = None;
    }
    fn observe(&mut self, snapshot: &Value, elapsed: Duration) -> bool {
        if self.snapshot.as_ref().is_some_and(|(previous, _, _)| {
            previous["me"] == snapshot["me"]
                && previous["active"]["id"] == snapshot["active"]["id"]
                && previous["active"]["revision"]
                    .as_u64()
                    .zip(snapshot["active"]["revision"].as_u64())
                    .is_some_and(|(old, new)| new < old)
        }) {
            return false;
        }
        if self
            .confirmation
            .as_ref()
            .is_some_and(|proof| !confirmation_matches(snapshot, proof))
        {
            self.confirmation = None;
        }
        self.snapshot = Some((snapshot.clone(), Instant::now(), elapsed));
        true
    }
}
static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
fn cache() -> &'static Mutex<Cache> {
    CACHE.get_or_init(|| Mutex::new(Cache::default()))
}

pub(crate) struct SnapshotAttempt<'a> {
    cache: &'a Mutex<Cache>,
    completed: bool,
}
impl SnapshotAttempt<'_> {
    pub(crate) fn complete(mut self) {
        self.completed = true;
    }
}
impl Drop for SnapshotAttempt<'_> {
    fn drop(&mut self) {
        if !self.completed {
            if let Ok(mut cache) = self.cache.lock() {
                cache.invalidate();
            }
        }
    }
}
pub(crate) fn snapshot_attempt() -> SnapshotAttempt<'static> {
    SnapshotAttempt {
        cache: cache(),
        completed: false,
    }
}

pub fn set_enabled(_enabled: bool) {
    if let Ok(mut c) = cache().lock() {
        *c = Cache::default();
    }
}
pub fn unavailable() {
    if let Ok(mut c) = cache().lock() {
        c.invalidate();
    }
}

/// Pass total request RTT; never renew freshness with a cached HTTP response.
pub fn observe_party(snapshot: &Value, elapsed: Duration) {
    let Ok(mut c) = cache().lock() else { return };
    c.observe(snapshot, elapsed);
}

/// Call only after real group validation and a successful draftConfirm reservation (returned Party).
pub fn confirmed(party: &Value) {
    let Ok(mut c) = cache().lock() else { return };
    let Some((snapshot, _, _)) = &c.snapshot else {
        return;
    };
    if party["id"] != snapshot["active"]["id"]
        || party["revision"] != snapshot["active"]["revision"]
    {
        return;
    }
    let Some(own) = party["members"]
        .as_array()
        .and_then(|ms| ms.iter().find(|m| m["id"] == snapshot["me"]))
    else {
        return;
    };
    let proof =
        serde_json::json!({"partyId":party["id"],"revision":party["revision"],"member":own});
    if confirmation_matches(snapshot, &proof) {
        c.confirmation = Some(proof);
    }
}

fn group_active() -> &'static AtomicBool {
    // Membership is runtime evidence; a saved flag cannot authorize or block a new session.
    static ACTIVE: AtomicBool = AtomicBool::new(false);
    &ACTIVE
}

pub fn is_enabled() -> bool {
    group_active().load(Ordering::SeqCst)
}

/// Only verified group membership may change this flag; transport failures retain it.
pub fn set_group_active(active: bool, settings: &crate::services::settings::SettingsService) {
    // Serialize persistence with transitions so concurrent observations cannot save out of order.
    static UPDATE: Mutex<()> = Mutex::new(());
    let Ok(_guard) = UPDATE.lock() else { return };
    update_group_active(group_active(), active, |value| {
        if let Err(error) = settings.save_setting(SETTING, &value) {
            log::error!("Cannot persist draft group activity: {error}");
        }
    });
}

fn update_group_active(active: &AtomicBool, value: bool, persist: impl FnOnce(bool)) {
    if active.swap(value, Ordering::SeqCst) != value {
        set_enabled(value);
        persist(value);
    }
}

fn lease_valid(snapshot: &Value, member: &Value, age: Duration) -> bool {
    let Some(now) = snapshot["serverNow"]
        .as_str()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
    else {
        return false;
    };
    let Some(seen) = member["seenAt"]
        .as_str()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
    else {
        return false;
    };
    let millis = now.signed_duration_since(seen).num_milliseconds();
    member["online"] == true
        && member["accepted"] == true
        && millis >= 0
        && (millis as u128 + age.as_millis()) < 20000
}

fn positive_id(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_str()?.parse().ok())
        .filter(|id| *id > 0)
}

fn identity_matches(player: &Value, own: &Value, local: &Value) -> bool {
    let Some(canonical) = own["gamePuuid"]
        .as_str()
        .filter(|p| crate::models::account::is_valid_puuid(p))
    else {
        return false;
    };
    if local["puuid"].as_str() != Some(canonical) {
        return false;
    }
    let raw = player["puuid"].as_str().unwrap_or("");
    if crate::models::account::is_valid_puuid(raw) {
        return raw == canonical;
    }
    let Some(summoner) = positive_id(&player["summonerId"]) else {
        return false;
    };
    let Some(riot) = own["gameRiotId"]
        .as_str()
        .filter(|s| !s.trim().is_empty() && s.contains('#'))
    else {
        return false;
    };
    positive_id(&own["gameSummonerId"]) == Some(summoner)
        && positive_id(&local["summonerId"]) == Some(summoner)
        && local["riotId"]
            .as_str()
            .is_some_and(|name| name.eq_ignore_ascii_case(riot))
}

/// None pauses coordination; Some(0) is reserved for explicitly disabled coordination.
pub fn choice(session: &Value, mode: &str) -> Option<i32> {
    if !is_enabled() {
        return Some(0);
    }
    let c = cache().lock().ok()?;
    let (s, at, rtt) = c.snapshot.as_ref()?;
    let age = at.elapsed() + *rtt;
    let proven = c
        .confirmation
        .as_ref()
        .is_some_and(|proof| confirmation_matches(s, proof));
    confirmed_choice(s, session, mode, age, proven)
}

fn confirmation_matches(s: &Value, proof: &Value) -> bool {
    let Some(party_id) = s["active"]["id"].as_str().filter(|id| !id.is_empty()) else {
        return false;
    };
    let Some(actor) = s["me"].as_str().filter(|id| !id.is_empty()) else {
        return false;
    };
    let Some(members) = s["active"]["members"].as_array() else {
        return false;
    };
    let mut own = members.iter().filter(|m| m["id"].as_str() == Some(actor));
    let member = own.next();
    if own.next().is_some() {
        return false;
    }
    proof["partyId"].as_str() == Some(party_id)
        && proof["revision"]
            .as_u64()
            .zip(s["active"]["revision"].as_u64())
            .is_some_and(|(a, b)| b >= a)
        && member.is_some_and(|m| {
            m["accepted"] == true
                && m["online"] == true
                && m["id"] == proof["member"]["id"]
                && m["gameSummonerId"] == proof["member"]["gameSummonerId"]
                && m["server"] == proof["member"]["server"]
                && m["gamePuuid"] == proof["member"]["gamePuuid"]
                && m["role"] == proof["member"]["role"]
                && m["secondaryRole"] == proof["member"]["secondaryRole"]
                && m["draft"] == proof["member"]["draft"]
                && m["draftChoice"] == proof["member"]["draftChoice"]
        })
}

fn confirmed_choice(
    s: &Value,
    session: &Value,
    mode: &str,
    age: Duration,
    armed: bool,
) -> Option<i32> {
    if age >= Duration::from_secs(5) {
        return None;
    }
    if s.get("active")?.is_null() {
        return None;
    }
    if !armed {
        return None;
    }
    let game = session["gameId"].as_i64().filter(|id| *id > 0)?.to_string();
    let player = super::champ_select_policy::player(session)?;
    let members = s["active"]["members"].as_array()?;
    if members.is_empty()
        || members.len() > 5
        || members
            .iter()
            .filter(|m| m["accepted"] == true)
            .any(|m| !lease_valid(s, m, age) || m["phase"] != "ChampSelect")
    {
        return None;
    }
    let actor = s["me"].as_str().filter(|id| !id.is_empty())?;
    let own = members.iter().find(|m| m["id"].as_str() == Some(actor))?;
    if !identity_matches(player, own, &s["local"]) || !lease_valid(s, own, age) {
        return None;
    }
    let draft = &own["draft"];
    if draft["sessionKey"].as_str() != Some(game.as_str()) || draft["mode"].as_str() != Some(mode) {
        return None;
    }
    let id = own["draftChoice"]
        .as_i64()
        .filter(|id| *id > 0 && *id <= 10000)?;
    if !draft["candidates"]
        .as_array()?
        .iter()
        .any(|v| v.as_i64() == Some(id))
    {
        return None;
    }
    let intent: DraftIntent = serde_json::from_value(draft.clone()).ok()?;
    if mode == "classic" {
        let (order, active) = classic_pick_turn(session)?;
        if !active
            || intent.pick_active != Some(true)
            || intent.pick_order != Some(order)
            || intent.pick_role.as_deref() != Some(super::champ_select_policy::role(player))
            || intent.pick_context != classic_pick_context(session)
            || intent.pick_context.is_none()
            || classic_occupied(session, id as i32)
        {
            return None;
        }
    }
    if !allowed_candidates(&intent.preferences.pick, &intent.candidates).contains(&(id as i32)) {
        return None;
    }
    if members.iter().any(|m| {
        m["id"] != s["me"]
            && m["draft"]["sessionKey"] == draft["sessionKey"]
            && m["draftChoice"].as_i64() == Some(id)
            && (mode != "classic"
                || (m["draft"]["pickActive"] == true
                    && m["draft"]["pickOrder"] == draft["pickOrder"]
                    && intent
                        .pick_context
                        .as_ref()
                        .is_some_and(|ctx| context_matches(m, ctx))))
    }) {
        return None;
    }
    Some(id as i32)
}

pub fn permits(session: &Value, mode: &str, id: i32, ban: bool) -> bool {
    if !ban {
        return choice(session, mode).is_some_and(|choice| choice == 0 || choice == id);
    }
    ban_is_safe(session, mode, id)
}

/// Safety authorization only; the caller exhausts configured bans before random fallback.
pub fn ban_is_safe(session: &Value, mode: &str, id: i32) -> bool {
    if !(1..=10000).contains(&id) || team_declares(session, id) {
        return false;
    }
    if !is_enabled() {
        return true;
    }
    let Ok(c) = cache().lock() else { return false };
    let Some((snapshot, at, rtt)) = &c.snapshot else {
        return false;
    };
    ban_allowed(snapshot, session, mode, id, at.elapsed() + *rtt)
}

fn team_declares(session: &Value, id: i32) -> bool {
    session["myTeam"].as_array().into_iter().flatten().any(|p| {
        p["championId"].as_i64() == Some(id as i64)
            || p["championPickIntent"].as_i64() == Some(id as i64)
    })
}

fn ban_allowed(snapshot: &Value, session: &Value, mode: &str, id: i32, age: Duration) -> bool {
    if age >= Duration::from_secs(5) || !(1..=10000).contains(&id) || team_declares(session, id) {
        return false;
    }
    let Some(game) = session["gameId"]
        .as_i64()
        .filter(|id| *id > 0)
        .map(|id| id.to_string())
    else {
        return false;
    };
    let Some(player) = super::champ_select_policy::player(session) else {
        return false;
    };
    let Some(members) = snapshot["active"]["members"].as_array() else {
        return false;
    };
    if members.len() < 2 || members.len() > 5 {
        return false;
    }
    let Some(own) = members.iter().find(|m| m["id"] == snapshot["me"]) else {
        return false;
    };
    if !identity_matches(player, own, &snapshot["local"])
        || !lease_valid(snapshot, own, age)
        || own["draftChoice"].as_i64() == Some(id as i64)
    {
        return false;
    }
    let Ok(intent) = serde_json::from_value::<DraftIntent>(own["draft"].clone()) else {
        return false;
    };
    if intent.session_key != game || intent.mode != mode {
        return false;
    }
    if members.iter().filter(|m| m["accepted"] == true).any(|m| {
        !lease_valid(snapshot, m, age)
            || m["phase"] != "ChampSelect"
            || m["draft"]["sessionKey"] != game
            || m["draft"]["mode"] != mode
            || serde_json::from_value::<DraftIntent>(m["draft"].clone()).is_err()
    }) {
        return false;
    }
    !members.iter().filter(|m| m["accepted"] == true).any(|m| {
        m["draftChoice"].as_i64() == Some(id as i64)
            || serde_json::from_value::<DraftIntent>(m["draft"].clone())
                .ok()
                .is_some_and(|d| {
                    d.preferences
                        .pick
                        .iter()
                        .any(|p| p.champion_id == id && p.state != PreferenceState::Exclude)
                })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn generated_group_plans_are_order_independent_and_never_assign_forbidden_or_duplicate_champions(
    ) {
        let mut seed = 913u32;
        for _ in 0..256 {
            let mut members = Vec::new();
            let mut allowed = BTreeMap::new();
            for index in 0..5 {
                let mut pool = Vec::new();
                let mut available = Vec::new();
                for champion in 1..=6 {
                    seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                    let state = ["must", "yield", "exclude"][(seed % 3) as usize];
                    pool.push(json!({"championId":champion,"state":state}));
                    if seed & 8 != 0 {
                        available.push(champion);
                    }
                }
                let online = seed & 32 != 0;
                let id = format!("player{index}");
                let green: Vec<_> = pool
                    .iter()
                    .filter(|p| {
                        p["state"] == "must"
                            && available.contains(&(p["championId"].as_i64().unwrap() as i32))
                    })
                    .map(|p| p["championId"].as_i64().unwrap() as i32)
                    .collect();
                let permitted: Vec<_> = if !online {
                    vec![]
                } else if !green.is_empty() {
                    green
                } else {
                    pool.iter()
                        .filter(|p| {
                            p["state"] == "yield"
                                && available.contains(&(p["championId"].as_i64().unwrap() as i32))
                        })
                        .map(|p| p["championId"].as_i64().unwrap() as i32)
                        .collect()
                };
                allowed.insert(id.clone(), permitted);
                members.push(json!({"id":id,"accepted":true,"online":online,"draftChoice":null,
                    "draft":{"sessionKey":"123","mode":"aram","candidates":available,"preferences":{"pick":pool}}}));
            }
            let plan = coordinate(&json!(members), "123", "aram");
            assert_eq!(plan.values().collect::<BTreeSet<_>>().len(), plan.len());
            for (player, champion) in &plan {
                assert!(allowed[player].contains(champion));
            }
            members.reverse();
            assert_eq!(coordinate(&json!(members), "123", "aram"), plan);
            assert!(coordinate(&json!(members), "124", "aram").is_empty());
            assert!(coordinate(&json!(members), "123", "mayhem").is_empty());
        }
    }
    #[tokio::test]
    async fn cancelled_snapshot_revokes_proof_but_success_preserves_it() {
        let cache = Mutex::new(Cache {
            snapshot: Some((json!({"active":{}}), Instant::now(), Duration::ZERO)),
            confirmation: Some(json!({"proof":true})),
        });
        let attempt = async {
            let _attempt = SnapshotAttempt {
                cache: &cache,
                completed: false,
            };
            std::future::pending::<()>().await;
        };
        let mut attempt = Box::pin(attempt);
        assert!(futures_util::poll!(&mut attempt).is_pending());
        assert!(cache.lock().unwrap().confirmation.is_some());
        drop(attempt);
        assert!(cache.lock().unwrap().snapshot.is_none());
        assert!(cache.lock().unwrap().confirmation.is_none());
        cache.lock().unwrap().confirmation = Some(json!({"new":true}));
        SnapshotAttempt {
            cache: &cache,
            completed: false,
        }
        .complete();
        assert_eq!(
            cache.lock().unwrap().confirmation,
            Some(json!({"new":true}))
        );
        {
            let _failed = SnapshotAttempt {
                cache: &cache,
                completed: false,
            };
        }
        assert!(cache.lock().unwrap().confirmation.is_none());
    }

    #[test]
    fn recovery_after_unavailability_requires_a_new_confirmation() {
        let (mut snapshot, _) = fixture();
        snapshot["active"]["id"] = json!("party");
        snapshot["active"]["revision"] = json!(7);
        let proof =
            json!({"partyId":"party","revision":7,"member":snapshot["active"]["members"][0]});
        assert!(confirmation_matches(&snapshot, &proof));
        let mut c = Cache::default();
        assert!(c.observe(&snapshot, Duration::ZERO));
        c.confirmation = Some(proof);
        c.invalidate();
        assert!(c.snapshot.is_none() && c.confirmation.is_none());
        c.invalidate();
        assert!(c.observe(&snapshot, Duration::ZERO));
        assert!(
            c.confirmation.is_none(),
            "fresh reads do not resurrect a revoked proof"
        );
    }

    #[test]
    fn stale_party_reply_cannot_replace_new_revision_or_renew_its_freshness() {
        let mut c = Cache::default();
        let newest =
            json!({"me":"me","active":{"id":"party","revision":10,"members":[{"id":"me"}]}});
        assert!(c.observe(&newest, Duration::from_millis(100)));
        let at = c.snapshot.as_ref().unwrap().1;
        let proof = json!({"partyId":"party","revision":10,"member":{"id":"me"}});
        c.confirmation = Some(proof.clone());
        let mut old = newest.clone();
        old["active"]["revision"] = json!(9);
        assert!(!c.observe(&old, Duration::ZERO));
        assert_eq!(c.snapshot.as_ref().unwrap().0, newest);
        assert_eq!(c.snapshot.as_ref().unwrap().1, at);
        assert_eq!(c.confirmation, Some(proof));
        let mut changed = newest.clone();
        changed["active"]["revision"] = json!(11);
        changed["active"]["members"][0]["role"] = json!("middle");
        assert!(c.observe(&changed, Duration::ZERO));
        assert!(c.confirmation.is_none());
        let mut another = newest;
        another["active"]["id"] = json!("other");
        another["active"]["revision"] = json!(1);
        assert!(
            c.observe(&another, Duration::ZERO),
            "new party has an independent revision sequence"
        );
    }
    fn planner_member(id: &str, preferences: Value, candidates: Value) -> Value {
        json!({"id":id,"accepted":true,"online":true,"draftChoice":null,
            "draft":{"sessionKey":"123","mode":"aram","candidates":candidates,
                "preferences":{"pick":preferences}}})
    }

    #[test]
    fn group_activity_starts_inactive_without_live_membership() {
        assert!(!is_enabled());
    }

    #[test]
    fn planner_ties_use_stable_ids_and_maximize_assignments_before_priority() {
        let pool = json!([{"championId":1,"state":"yield"},{"championId":2,"state":"yield"}]);
        let a = planner_member("a", pool.clone(), json!([1, 2]));
        let b = planner_member("b", pool, json!([1, 2]));
        let expected = BTreeMap::from([("a".into(), 1), ("b".into(), 2)]);
        assert_eq!(
            coordinate(&json!([b.clone(), a.clone()]), "123", "aram"),
            expected
        );
        assert_eq!(coordinate(&json!([a.clone(), b]), "123", "aram"), expected);
        let b = planner_member("b", json!([{"championId":1,"state":"must"}]), json!([1]));
        assert_eq!(
            coordinate(&json!([a, b]), "123", "aram"),
            BTreeMap::from([("a".into(), 2), ("b".into(), 1)])
        );
    }

    #[test]
    fn planner_honors_strict_exclusions_and_preference_order() {
        let mut a = planner_member(
            "a",
            json!([
            {"championId":1,"state":"must"},{"championId":2,"state":"yield"},
            {"championId":3,"state":"exclude"}]),
            json!([2, 3]),
        );
        assert_eq!(
            coordinate(&json!([a.clone()]), "123", "aram"),
            BTreeMap::from([("a".into(), 2)])
        );
        a["draft"]["candidates"] = json!([1, 2, 3]);
        let b = planner_member("b", json!([{"championId":1,"state":"yield"}]), json!([1]));
        assert_eq!(
            coordinate(&json!([b, a]), "123", "aram"),
            BTreeMap::from([("a".into(), 1)])
        );
        let c = planner_member(
            "c",
            json!([{"championId":2,"state":"yield"},{"championId":1,"state":"yield"}]),
            json!([1, 2]),
        );
        assert_eq!(
            coordinate(&json!([c]), "123", "aram"),
            BTreeMap::from([("c".into(), 2)])
        );
    }

    #[test]
    fn planner_keeps_existing_claims_and_rejects_conflicts() {
        let mut a = planner_member(
            "a",
            json!([{"championId":1,"state":"yield"},{"championId":2,"state":"yield"}]),
            json!([1, 2]),
        );
        a["draftChoice"] = json!(2);
        let mut b = a.clone();
        b["id"] = json!("b");
        b["draftChoice"] = Value::Null;
        assert_eq!(
            coordinate(&json!([a.clone(), b.clone()]), "123", "aram"),
            BTreeMap::from([("a".into(), 2), ("b".into(), 1)])
        );
        b["draftChoice"] = json!(2);
        assert!(coordinate(&json!([a.clone(), b]), "123", "aram").is_empty());
        a["draftChoice"] = json!(3);
        assert!(coordinate(&json!([a]), "123", "aram").is_empty());
    }

    #[test]
    fn planner_rejects_invalid_and_oversized_inputs_and_filters_sessions() {
        let a = planner_member("a", json!([{"championId":1,"state":"yield"}]), json!([1]));
        for key in ["", "0", "01", "-1", "1x"] {
            assert!(coordinate(&json!([a.clone()]), key, "aram").is_empty());
        }
        assert!(coordinate(&Value::Null, "123", "aram").is_empty());
        assert!(coordinate(&json!([a.clone(), a.clone()]), "123", "aram").is_empty());
        let many: Vec<_> = (0..6)
            .map(|i| {
                let mut m = a.clone();
                m["id"] = json!(i.to_string());
                m
            })
            .collect();
        assert!(coordinate(&json!(many), "123", "aram").is_empty());
        assert!(coordinate(&json!([a.clone()]), "124", "aram").is_empty());
        assert!(coordinate(&json!([a.clone()]), "123", "classic").is_empty());
        for available in [json!([0]), json!([10001]), json!([1.5]), json!(vec![1; 31])] {
            let mut m = a.clone();
            m["draft"]["candidates"] = available;
            assert!(coordinate(&json!([m]), "123", "aram").is_empty());
        }
        for pool in [
            json!([{"championId":1,"state":"invalid"}]),
            json!([{"championId":1,"state":"yield"},{"championId":1,"state":"yield"}]),
            json!((1..=31)
                .map(|id| json!({"championId":id,"state":"yield"}))
                .collect::<Vec<_>>()),
        ] {
            let mut m = a.clone();
            m["draft"]["preferences"]["pick"] = pool;
            assert!(coordinate(&json!([m]), "123", "aram").is_empty());
        }
        let mut offline = a;
        offline["online"] = json!(false);
        assert!(coordinate(&json!([offline]), "123", "aram").is_empty());
    }

    #[test]
    fn group_activity_persists_only_transitions_and_preference_reset_cannot_activate_it() {
        let active = AtomicBool::new(true);
        let mut saved = Vec::new();
        update_group_active(&active, true, |v| saved.push(v));
        update_group_active(&active, false, |v| saved.push(v));
        update_group_active(&active, false, |v| saved.push(v));
        set_enabled(true);
        assert!(!active.load(Ordering::SeqCst));
        update_group_active(&active, true, |v| saved.push(v));
        unavailable();
        assert!(active.load(Ordering::SeqCst));
        assert_eq!(saved, vec![false, true]);
    }
    fn fixture() -> (Value, Value) {
        (
            json!({"me":"root", "serverNow":"2026-11-05T10:00:00Z", "local":{"puuid":"a".repeat(78)}, "active":{"members":[{
                "id":"root", "accepted":true,"online":true,"phase":"ChampSelect","seenAt":"2026-11-05T09:59:59Z","gamePuuid":"a".repeat(78),
                "draft":{"sessionKey":"123","mode":"classic","pickOrder":0,"pickActive":true,"pickRole":"middle","pickContext":[{"puuid":"a".repeat(78),"summonerId":"","pickOrder":0,"pickActive":true,"pickRole":"middle"}],"candidates":[1,2],"banCandidates":[3],"preferences":{"pick":[{"championId":1,"state":"yield"},{"championId":2,"state":"yield"}],"ban":[{"championId":3,"state":"yield"}]}},"draftChoice":2
            }]}}),
            json!({"gameId":123,"localPlayerCellId":0,"actions":[[{"type":"pick","actorCellId":0,"isInProgress":true,"completed":false}]],"myTeam":[{"cellId":0,"assignedPosition":"middle","puuid":"a".repeat(78)}]}),
        )
    }
    #[test]
    fn ban_pool_falls_back_when_actual_peer_wants_lux_before_picks() {
        let (mut s, mut session) = fixture();
        let own = &mut s["active"]["members"][0];
        own["draftChoice"] = Value::Null;
        own["draft"]["candidates"] = json!([]);
        own["draft"]["preferences"]["pick"] = json!([]);
        own["draft"]["banCandidates"] = json!([99, 3, 4]);
        own["draft"]["preferences"]["ban"] = json!([
            {"championId":99,"state":"yield"},
            {"championId":3,"state":"yield"},
            {"championId":4,"state":"yield"}]);
        let mut peer = own.clone();
        peer["id"] = json!("peer");
        peer["gamePuuid"] = json!("b".repeat(78));
        session["myTeam"]
            .as_array_mut()
            .unwrap()
            .push(json!({"cellId":1,"puuid":"b".repeat(78),"championId":0,"championPickIntent":0}));
        s["active"]["members"].as_array_mut().unwrap().push(peer);
        for state in ["must", "yield"] {
            s["active"]["members"][1]["draft"]["preferences"]["pick"] =
                json!([{"championId":99,"state":state}]);
            let selected = [99, 3, 4]
                .into_iter()
                .find(|id| ban_allowed(&s, &session, "classic", *id, Duration::ZERO));
            assert_eq!(selected, Some(3));
        }
        s["active"]["members"][1]["draft"]["preferences"]["pick"] =
            json!([{"championId":99,"state":"exclude"}]);
        assert!(ban_allowed(&s, &session, "classic", 99, Duration::ZERO));
        s["active"]["members"][1]["draft"] = Value::Null;
        assert!(!ban_allowed(&s, &session, "classic", 3, Duration::ZERO));
    }

    #[test]
    fn random_outside_configured_bans_requires_known_group_and_protects_all_peer_greens() {
        let (mut s, mut session) = fixture();
        let mut peer = s["active"]["members"][0].clone();
        peer["id"] = json!("peer");
        peer["draftChoice"] = json!(99);
        peer["draft"]["preferences"]["pick"] = json!([
            {"championId":99,"state":"must"}, {"championId":4,"state":"must"}]);
        s["active"]["members"].as_array_mut().unwrap().push(peer);
        assert!(ban_allowed(&s, &session, "classic", 5, Duration::ZERO));
        assert!(!ban_allowed(&s, &session, "classic", 4, Duration::ZERO));
        assert!(!ban_allowed(&s, &session, "classic", 1, Duration::ZERO));
        session["myTeam"]
            .as_array_mut()
            .unwrap()
            .push(json!({"cellId":1,"championPickIntent":5}));
        assert!(!ban_allowed(&s, &session, "classic", 5, Duration::ZERO));
        session["myTeam"][1]["championPickIntent"] = json!(0);
        s["active"]["members"][1]["draft"]["preferences"] = Value::Null;
        assert!(!ban_allowed(&s, &session, "classic", 5, Duration::ZERO));
        assert!(!ban_allowed(
            &Value::Null,
            &session,
            "classic",
            5,
            Duration::ZERO
        ));
    }

    #[test]
    fn ban_only_automation_needs_no_fake_pick_reservation_and_protects_peer_picks() {
        let (mut s, session) = fixture();
        s["active"]["members"][0]["draftChoice"] = Value::Null;
        s["active"]["members"][0]["draft"]["candidates"] = json!([]);
        s["active"]["members"][0]["draft"]["preferences"]["pick"] = json!([]);
        let mut peer = s["active"]["members"][0].clone();
        peer["id"] = json!("peer");
        s["active"]["members"].as_array_mut().unwrap().push(peer);
        assert!(ban_allowed(&s, &session, "classic", 3, Duration::ZERO));
        assert!(!ban_allowed(
            &s,
            &session,
            "classic",
            3,
            Duration::from_secs(5)
        ));
        s["active"]["members"][1]["draft"]["preferences"]["pick"] =
            json!([{"championId":3,"state":"yield"}]);
        assert!(!ban_allowed(&s, &session, "classic", 3, Duration::ZERO));
        s["active"]["members"][1]["draft"]["preferences"]["pick"] =
            json!([{"championId":1,"state":"must"},{"championId":3,"state":"yield"}]);
        assert!(!ban_allowed(&s, &session, "classic", 3, Duration::ZERO));
        s["active"]["members"][1]["draft"]["preferences"]["pick"] =
            json!([{"championId":3,"state":"exclude"}]);
        assert!(ban_allowed(&s, &session, "classic", 3, Duration::ZERO));
        s["active"]["members"][1]["draft"]["sessionKey"] = json!("124");
        assert!(!ban_allowed(&s, &session, "classic", 3, Duration::ZERO));
    }
    #[test]
    fn fresh_role_order_activity_and_occupied_changes_reject_old_confirmation() {
        let (s, session) = fixture();
        let mut role = session.clone();
        role["myTeam"][0]["assignedPosition"] = json!("top");
        assert_eq!(
            confirmed_choice(&s, &role, "classic", Duration::ZERO, true),
            None
        );
        let mut order = session.clone();
        order["actions"]
            .as_array_mut()
            .unwrap()
            .insert(0, json!([]));
        assert_eq!(
            confirmed_choice(&s, &order, "classic", Duration::ZERO, true),
            None
        );
        let mut inactive = session.clone();
        inactive["actions"][0][0]["isInProgress"] = json!(false);
        assert_eq!(
            confirmed_choice(&s, &inactive, "classic", Duration::ZERO, true),
            None
        );
        for team in ["myTeam", "theirTeam"] {
            let mut occupied = session.clone();
            if occupied[team].is_null() {
                occupied[team] = json!([]);
            }
            occupied[team]
                .as_array_mut()
                .unwrap()
                .push(json!({"cellId":9,"championId":2}));
            assert_eq!(
                confirmed_choice(&s, &occupied, "classic", Duration::ZERO, true),
                None
            );
        }
    }
    #[test]
    fn confirmation_requires_fresh_cache_and_new_enable_proof() {
        let (s, session) = fixture();
        assert_eq!(
            confirmed_choice(&s, &session, "classic", Duration::ZERO, true),
            Some(2)
        );
        assert_eq!(
            confirmed_choice(&s, &session, "classic", Duration::from_secs(5), true),
            None
        );
        assert_eq!(
            confirmed_choice(&s, &session, "classic", Duration::ZERO, false),
            None
        );
        assert_eq!(
            confirmed_choice(&s, &session, "aram", Duration::ZERO, true),
            None
        );
    }
    #[test]
    fn game_and_actor_identity_cannot_reuse_a_confirmation() {
        let (s, mut session) = fixture();
        for game in [0, 124] {
            session["gameId"] = json!(game);
            assert_eq!(
                confirmed_choice(&s, &session, "classic", Duration::ZERO, true),
                None
            );
        }
        session["gameId"] = json!(123);
        session["myTeam"][0]["puuid"] = json!("other");
        assert_eq!(
            confirmed_choice(&s, &session, "classic", Duration::ZERO, true),
            None
        );
    }
    #[test]
    fn leases_rtt_invalid_ids_and_candidates_fail_closed() {
        let (mut s, session) = fixture();
        s["active"]["members"][0]["seenAt"] = json!("2026-11-05T09:59:41Z");
        assert_eq!(
            confirmed_choice(&s, &session, "classic", Duration::from_secs(1), true),
            None
        );
        s["active"]["members"][0]["seenAt"] = json!("2026-11-05T09:59:59Z");
        for id in [0i64, -1, 2147483648, 3] {
            s["active"]["members"][0]["draftChoice"] = json!(id);
            assert_eq!(
                confirmed_choice(&s, &session, "classic", Duration::ZERO, true),
                None
            );
        }
    }

    #[test]
    fn guid_fallback_requires_both_positive_summoner_bindings_and_matching_riot_identity() {
        let (mut s, mut session) = fixture();
        session["myTeam"][0]["puuid"] = json!("3e863f7c-7282-48a1-8680-dfc482bceeed");
        session["myTeam"][0]["summonerId"] = json!(12345);
        s["local"]["summonerId"] = json!("12345");
        s["local"]["riotId"] = json!("Player#RU");
        s["active"]["members"][0]["gameSummonerId"] = json!("12345");
        s["active"]["members"][0]["gameRiotId"] = json!("Player#RU");
        s["active"]["members"][0]["draft"]["pickContext"][0]["puuid"] =
            session["myTeam"][0]["puuid"].clone();
        s["active"]["members"][0]["draft"]["pickContext"][0]["summonerId"] = json!("12345");
        assert_eq!(
            confirmed_choice(&s, &session, "classic", Duration::ZERO, true),
            Some(2)
        );
        let mut wrong = session.clone();
        wrong["myTeam"][0]["summonerId"] = json!(999);
        assert_eq!(
            confirmed_choice(&s, &wrong, "classic", Duration::ZERO, true),
            None
        );
        wrong["myTeam"][0]["summonerId"] = json!(0);
        assert_eq!(
            confirmed_choice(&s, &wrong, "classic", Duration::ZERO, true),
            None
        );
        s["local"]["riotId"] = json!("Other#RU");
        assert_eq!(
            confirmed_choice(&s, &session, "classic", Duration::ZERO, true),
            None
        );
    }

    #[test]
    fn valid_canonical_puuid_mismatch_never_uses_summoner_fallback() {
        let (mut s, mut session) = fixture();
        session["myTeam"][0]["puuid"] = json!("b".repeat(78));
        session["myTeam"][0]["summonerId"] = json!(123);
        s["local"]["summonerId"] = json!("123");
        s["local"]["riotId"] = json!("Player#RU");
        s["active"]["members"][0]["gameSummonerId"] = json!("123");
        s["active"]["members"][0]["gameRiotId"] = json!("Player#RU");
        assert_eq!(
            confirmed_choice(&s, &session, "classic", Duration::ZERO, true),
            None
        );
    }

    #[test]
    fn confirmation_is_bound_to_actor_party_revision_intent_and_choice() {
        let (mut s, _) = fixture();
        s["active"]["id"] = json!("party");
        s["active"]["revision"] = json!(8);
        let proof = json!({"partyId":"party","revision":8,"member":s["active"]["members"][0]});
        assert!(confirmation_matches(&s, &proof));
        s["active"]["revision"] = json!(9);
        assert!(confirmation_matches(&s, &proof));
        let mut duplicate = s.clone();
        duplicate["active"]["members"]
            .as_array_mut()
            .unwrap()
            .push(s["active"]["members"][0].clone());
        assert!(!confirmation_matches(&duplicate, &proof));
        for invalid in [Value::Null, json!(""), json!(7)] {
            let mut malformed = s.clone();
            malformed["active"]["id"] = invalid.clone();
            let mut bad_proof = proof.clone();
            bad_proof["partyId"] = invalid;
            assert!(!confirmation_matches(&malformed, &bad_proof));
        }
        for (field, value) in [
            ("accepted", json!(false)),
            ("online", json!(false)),
            ("server", json!("EUW")),
            ("gameSummonerId", json!("99")),
            ("gamePuuid", json!("b".repeat(78))),
            ("role", json!("middle")),
            ("secondaryRole", json!("utility")),
            ("draftChoice", Value::Null),
            (
                "draft",
                json!({"sessionKey":"124","mode":"classic","candidates":[1,2]}),
            ),
        ] {
            let mut bad = s.clone();
            bad["active"]["members"][0][field] = value;
            assert!(!confirmation_matches(&bad, &proof));
        }
        s["active"]["id"] = json!("other-party");
        assert!(!confirmation_matches(&s, &proof));
    }

    #[test]
    fn unavailable_green_falls_back_to_yellow_but_never_excluded() {
        let pool = vec![
            ChampionPreference {
                champion_id: 1,
                state: PreferenceState::Must,
            },
            ChampionPreference {
                champion_id: 2,
                state: PreferenceState::Yield,
            },
            ChampionPreference {
                champion_id: 3,
                state: PreferenceState::Exclude,
            },
        ];
        assert_eq!(allowed_candidates(&pool, &[2, 3]), vec![2]);
        assert_eq!(allowed_candidates(&pool, &[1, 2, 3]), vec![1]);
        let strict = vec![
            ChampionPreference {
                champion_id: 2,
                state: PreferenceState::Must,
            },
            ChampionPreference {
                champion_id: 1,
                state: PreferenceState::Must,
            },
        ];
        assert_eq!(allowed_candidates(&strict, &[1, 2]), vec![2, 1]);
    }

    #[test]
    fn malformed_candidate_lists_cannot_override_exclusion_in_any_mode() {
        for mode in ["classic", "aram", "mayhem", "arena"] {
            let (mut s, session) = fixture();
            s["active"]["members"][0]["draft"]["mode"] = json!(mode);
            s["active"]["members"][0]["draft"]["preferences"]["pick"][1]["state"] =
                json!("exclude");
            assert_eq!(
                confirmed_choice(&s, &session, mode, Duration::ZERO, true),
                None
            );
        }
    }
}

#[cfg(test)]
mod available_green_tests {
    use super::*;
    use serde_json::json;

    fn member(id: &str, preferences: Value, candidates: Value) -> Value {
        json!({"id":id,"accepted":true,"online":true,"draft":{"sessionKey":"123","mode":"aram","preferences":{"pick":preferences},"candidates":candidates},"draftChoice":null})
    }

    #[test]
    fn fallback_yellow_does_not_receive_unavailable_green_score_bonus() {
        let members = json!([
            member(
                "a",
                json!([{"championId":1,"state":"must"},{"championId":2,"state":"yield"}]),
                json!([2])
            ),
            member("b", json!([{"championId":2,"state":"must"}]), json!([2]))
        ]);
        assert_eq!(
            coordinate(&members, "123", "aram"),
            BTreeMap::from([("b".into(), 2)])
        );
    }

    #[test]
    fn peer_reservation_is_not_physical_unavailability_and_never_unlocks_yellow() {
        let mut members = json!([
            member(
                "a",
                json!([{"championId":1,"state":"must"},{"championId":2,"state":"yield"}]),
                json!([1, 2])
            ),
            member("b", json!([{"championId":1,"state":"must"}]), json!([1]))
        ]);
        members[1]["draftChoice"] = json!(1);
        assert_eq!(
            coordinate(&members, "123", "aram"),
            BTreeMap::from([("b".into(), 1)])
        );
        members[0]["draft"]["candidates"] = json!([2]);
        assert_eq!(
            coordinate(&members, "123", "aram"),
            BTreeMap::from([("a".into(), 2), ("b".into(), 1)])
        );
    }
}

#[cfg(test)]
mod ban_strictness_tests {
    use super::*;
    #[test]
    fn legacy_ban_states_never_block_second_or_third_priority() {
        let pool = vec![
            ChampionPreference {
                champion_id: 1,
                state: PreferenceState::Must,
            },
            ChampionPreference {
                champion_id: 2,
                state: PreferenceState::Yield,
            },
            ChampionPreference {
                champion_id: 3,
                state: PreferenceState::Exclude,
            },
        ];
        assert_eq!(allowed_candidates(&pool, &[2, 3]), vec![2]);
        assert_eq!(allowed_ban_candidates(&pool, &[2, 3]), vec![2, 3]);
        assert_eq!(allowed_ban_candidates(&pool, &[3]), vec![3]);
        assert_eq!(allowed_ban_candidates(&pool, &[1, 2, 3]), vec![1, 2, 3]);
        assert_eq!(allowed_ban_candidates(&pool, &[1]), vec![1]);
    }
}

#[cfg(test)]
mod fairness_tests {
    use super::*;
    use serde_json::json;

    fn member(id: &str, ids: &[i32]) -> Value {
        json!({"id":id,"accepted":true,"online":true,"draftChoice":null,
            "draft":{"sessionKey":"123","mode":"aram","candidates":ids,
            "preferences":{"pick":ids.iter().map(|id| json!({"championId":id,"state":"must"})).collect::<Vec<_>>()}}})
    }

    #[test]
    fn shared_turn_is_order_independent_and_only_server_outcomes_rotate_it() {
        let a = member("a", &[99]);
        let mut b = member("b", &[99]);
        assert_eq!(
            coordinate(&json!([a, b]), "123", "aram"),
            BTreeMap::from([("a".into(), 99)])
        );
        // A proposal or cancellation leaves the authoritative ledger untouched.
        assert_eq!(
            coordinate(&json!([b, a]), "123", "aram"),
            BTreeMap::from([("a".into(), 99)])
        );
        b["greenPriority"] = json!([{"championId":99,"againstId":"a"}]);
        for ms in [json!([a, b]), json!([b, a])] {
            assert_eq!(
                coordinate(&ms, "123", "aram"),
                BTreeMap::from([("b".into(), 99)])
            );
        }
    }

    #[test]
    fn alternatives_override_rotation_and_riot_account_switch_keeps_turn() {
        let mut a = member("a", &[99, 103]);
        let b = member("b", &[99]);
        a["greenPriority"] = json!([{"championId":99,"againstId":"b"}]);
        assert_eq!(
            coordinate(&json!([a, b]), "123", "aram"),
            BTreeMap::from([("a".into(), 103), ("b".into(), 99)])
        );
        a["draft"]["candidates"] = json!([99]);
        a["gamePuuid"] = json!("switched-account");
        assert_eq!(
            coordinate(&json!([b, a]), "123", "aram"),
            BTreeMap::from([("a".into(), 99)])
        );
    }
}

#[cfg(test)]
mod classic_turn_tests {
    use super::*;
    use serde_json::json;
    fn member(id: &str, ids: &[i32], order: usize, active: bool) -> Value {
        json!({"id":id,"accepted":true,"online":true,"draftChoice":null,
            "draft":{"sessionKey":"123","mode":"classic","pickOrder":order,"pickActive":active,
            "candidates":ids,"preferences":{"pick":ids.iter().map(|id|json!({"championId":id,"state":"must"})).collect::<Vec<_>>()}}})
    }
    #[test]
    fn earlier_real_turn_gets_lux_before_future_sole_lux_even_with_rotation_or_claim() {
        let a = member("z", &[99, 103], 1, true);
        let mut b = member("a", &[99], 2, false);
        b["draftChoice"] = json!(99);
        b["greenPriority"] = json!([{"championId":99,"againstId":"z"}]);
        assert_eq!(
            coordinate(&json!([b, a]), "123", "classic"),
            BTreeMap::from([("z".into(), 99)])
        );
    }
    #[test]
    fn live_context_drops_peer_old_order_or_role_claim_without_waiting_for_heartbeat() {
        let mut a = member("z", &[99, 103], 1, true);
        let mut b = member("a", &[99], 1, true);
        a["gamePuuid"] = json!("z".repeat(78));
        b["gamePuuid"] = json!("a".repeat(78));
        a["draft"]["pickRole"] = json!("middle");
        b["draft"]["pickRole"] = json!("middle");
        b["draftChoice"] = json!(99);
        a["draft"]["pickContext"] = json!([
            {"puuid":"z".repeat(78),"summonerId":"","pickOrder":1,"pickActive":true,"pickRole":"middle"},
            {"puuid":"a".repeat(78),"summonerId":"","pickOrder":2,"pickActive":false,"pickRole":"top"}]);
        let expected = BTreeMap::from([("z".into(), 99)]);
        assert_eq!(
            coordinate_for_actor(&json!([b, a]), "123", "classic", "z"),
            expected
        );
        a["draft"]["pickContext"][1]["pickOrder"] = json!(1);
        a["draft"]["pickContext"][1]["pickActive"] = json!(true);
        assert_eq!(
            coordinate_for_actor(&json!([b, a]), "123", "classic", "z"),
            expected
        );
    }
    #[test]
    fn simultaneous_current_group_matches_without_duplicate_or_claim_race() {
        let a = member("z", &[99, 103], 2, true);
        let b = member("a", &[99], 2, true);
        let expected = BTreeMap::from([("a".into(), 99), ("z".into(), 103)]);
        assert_eq!(coordinate(&json!([a, b]), "123", "classic"), expected);
        assert_eq!(coordinate(&json!([b, a]), "123", "classic"), expected);
    }
    #[test]
    fn turn_uses_action_group_and_actor_not_team_row_and_bans_precede_picks() {
        let mut session = json!({"localPlayerCellId":7,"myTeam":[{"cellId":8},{"cellId":7}],
            "actions":[[{"actorCellId":7,"type":"ban","completed":false,"isInProgress":true}],
            [{"actorCellId":8,"type":"pick","completed":false,"isInProgress":true}],
            [{"actorCellId":7,"type":"pick","completed":false,"isInProgress":true}]]});
        assert_eq!(classic_pick_turn(&session), Some((2, false)));
        session["actions"][0][0]["completed"] = json!(true);
        assert_eq!(classic_pick_turn(&session), Some((2, true)));
        session["actions"].as_array_mut().unwrap().swap(1, 2);
        assert_eq!(classic_pick_turn(&session), Some((1, true)));
        session["actions"][1][0]["isInProgress"] = json!(false);
        assert_eq!(classic_pick_turn(&session), Some((1, false)));
    }
    #[test]
    fn ally_enemy_picks_and_completed_bans_remove_candidates_but_hover_does_not() {
        let session = json!({"localPlayerCellId":1,"myTeam":[{"cellId":1,"championId":99},{"cellId":2,"championId":103,"championPickIntent":5}],
            "theirTeam":[{"cellId":6,"championId":4}],"actions":[[{"type":"ban","completed":true,"championId":3}]]});
        for id in [103, 4, 3] {
            assert!(classic_occupied(&session, id));
        }
        for id in [99, 5] {
            assert!(!classic_occupied(&session, id));
        }
    }
}
