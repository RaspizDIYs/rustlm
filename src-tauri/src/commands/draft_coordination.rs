use crate::{
    error::AppError, models::automation::AutomationSettings,
    services::champ_select_policy as policy,
};
use crate::{services::draft_coordination as draft, state::AppState};
pub use draft::{ChampionPreference, DraftIntent, DraftPreferences, PreferenceState};
use serde::Serialize;
use serde_json::Value;
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};
use tauri::State;

const SHARE: &str = "DraftCoordinationShare";
const PREFERENCES: &str = "DraftCoordinationPreferences";
type SavedPreferences = BTreeMap<String, DraftPreferences>;
static EDIT: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftLocal {
    #[serde(flatten)]
    pub intent: DraftIntent,
    pub role: String,
    pub auto_pick_enabled: bool,
    pub auto_ban_enabled: bool,
    pub coordination_status: String,
    pub champion_names: BTreeMap<i32, String>,
    pub palette_preferences: DraftPreferences,
    pub marked_preferences: BTreeMap<String, Vec<i32>>,
}

fn profile_key(mode: &str, role: &str) -> String {
    if mode == "classic" {
        format!("classic:{role}")
    } else {
        mode.into()
    }
}

fn configured(
    settings: &AutomationSettings,
    saved: &SavedPreferences,
    mode: &str,
    role: &str,
) -> DraftLocal {
    let key = profile_key(mode, role);
    let effective = settings.for_queue(mode, role);
    let profile = settings.profiles.get(&key);
    let picks = if matches!(mode, "aram" | "mayhem") {
        profile
            .map(|p| p.active_pick_ids().to_vec())
            .unwrap_or_default()
    } else {
        [
            effective.pick_champion1_id,
            effective.pick_champion2_id,
            effective.pick_champion3_id,
        ]
        .into_iter()
        .flatten()
        .collect()
    };
    let bans = if matches!(mode, "classic" | "arena") {
        effective.ban_champion_ids.clone()
    } else {
        vec![]
    };
    let pool = |ids: Vec<i32>, kind: &str| {
        let overrides = saved.get(&key).filter(|_| kind == "pick").map(|p| &p.pick);
        let mut result = Vec::<ChampionPreference>::new();
        for id in ids {
            if !(1..=10000).contains(&id) || result.iter().any(|p| p.champion_id == id) {
                continue;
            }
            result.push(ChampionPreference {
                champion_id: id,
                state: overrides
                    .and_then(|ps| ps.iter().find(|p| p.champion_id == id))
                    .map(|p| p.state)
                    .unwrap_or(PreferenceState::Yield),
            });
            if result.len() == 30 {
                break;
            }
        }
        result
    };
    let mut names = BTreeMap::new();
    if let Some(p) = profile {
        for (id, name) in p.pick_ids.iter().zip(&p.picks) {
            if *id > 0 {
                names.insert(*id, name.clone());
            }
        }
    }
    for (id, name) in [
        (
            effective.pick_champion1_id,
            effective.pick_champion1.as_ref(),
        ),
        (
            effective.pick_champion2_id,
            effective.pick_champion2.as_ref(),
        ),
        (
            effective.pick_champion3_id,
            effective.pick_champion3.as_ref(),
        ),
    ] {
        if let (Some(id), Some(name)) = (id, name) {
            if id > 0 && !name.is_empty() {
                names.entry(id).or_insert_with(|| name.clone());
            }
        }
    }
    if let Some(p) = profile {
        for (id, name) in p.ban_ids.iter().zip(&p.bans) {
            if *id > 0 {
                names.insert(*id, name.clone());
            }
        }
    }
    if let (Some(id), Some(name)) = (effective.ban_champion_id, effective.ban_champion) {
        names.insert(id, name);
    }
    DraftLocal {
        intent: DraftIntent {
            session_key: String::new(),
            mode: mode.into(),
            candidates: vec![],
            ban_candidates: vec![],
            pick_order: None,
            pick_active: None,
            pick_role: None,
            pick_context: None,
            preferences: DraftPreferences {
                pick: pool(picks, "pick"),
                ban: pool(bans, "ban"),
            },
        },
        role: if mode == "classic" {
            role.into()
        } else {
            String::new()
        },
        auto_pick_enabled: effective.auto_pick_enabled,
        auto_ban_enabled: effective.auto_ban_enabled && matches!(mode, "classic" | "arena"),
        coordination_status: "waiting".into(),
        champion_names: names,
        palette_preferences: DraftPreferences {
            pick: saved.get(&key).map(|p| p.pick.clone()).unwrap_or_default(),
            ban: vec![],
        },
        marked_preferences: BTreeMap::from([
            (
                "pick".into(),
                saved
                    .get(&key)
                    .map(|p| p.pick.iter().map(|v| v.champion_id).collect())
                    .unwrap_or_default(),
            ),
            ("ban".into(), vec![]),
        ]),
    }
}

async fn lcu(state: &AppState, path: &str) -> Result<Value, AppError> {
    Ok(serde_json::from_str(
        &state.riot_client.lcu_get(path).await?,
    )?)
}

fn live_candidates(
    session: &Value,
    mode: &str,
    preferences: &[i32],
    ids: &[i32],
    subset: &[i32],
    bench: bool,
) -> Vec<i32> {
    let Some(me) = policy::player(session) else {
        return vec![];
    };
    preferences
        .iter()
        .copied()
        .filter(|id| {
            let banned = session["actions"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_array)
                .flatten()
                .any(|a| {
                    a["type"] == "ban"
                        && a["completed"] == true
                        && a["championId"].as_i64() == Some(*id as i64)
                });
            !banned
                && (mode != "classic" || !draft::classic_occupied(session, *id))
                && if matches!(mode, "aram" | "mayhem") {
                    me["championId"].as_i64() == Some(*id as i64)
                        || policy::bench_pick(session, &[*id], ids, subset, bench) == Some(*id)
                } else {
                    ids.contains(id)
                        && (session["allowSubsetChampionPicks"] != true || subset.contains(id))
                        && !session["myTeam"].as_array().is_some_and(|team| {
                            team.iter().any(|p| {
                                p["cellId"] != session["localPlayerCellId"]
                                    && (mode != "arena" || policy::same_pick_team(session, p))
                                    && p["championId"].as_i64() == Some(*id as i64)
                            })
                        })
                }
        })
        .collect()
}

/// Own live candidates only. Full configured pools remain visible when unavailable.
pub async fn local(state: &AppState) -> Result<DraftLocal, AppError> {
    let started = Instant::now();
    let session = lcu(state, "/lol-champ-select/v1/session").await?;
    let flow = lcu(state, "/lol-gameflow/v1/session").await?;
    let mode = policy::mode(&flow)
        .ok_or_else(|| AppError::Custom("Сейчас нет выбора чемпионов".into()))?;
    let me = policy::player(&session)
        .ok_or_else(|| AppError::Custom("Игровой аккаунт не определён".into()))?;
    let role = policy::role(me);
    let settings = state.auto_accept.get_settings().await;
    let saved: SavedPreferences = state.settings.load_setting(PREFERENCES, BTreeMap::new());
    let mut result = configured(&settings, &saved, mode, role);
    let effective = settings.for_queue(mode, role);
    let key = profile_key(mode, role);
    let profile = settings.profiles.get(&key);
    let preferences: Vec<i32> = result
        .intent
        .preferences
        .pick
        .iter()
        .map(|p| p.champion_id)
        .collect();
    let mut ids: Vec<i32> =
        serde_json::from_value(lcu(state, "/lol-champ-select/v1/pickable-champion-ids").await?)?;
    if mode == "arena" && profile.is_some_and(|p| p.arena_pick == "crowd") {
        let favorites: Vec<i32> = serde_json::from_value(
            lcu(
                state,
                "/lol-lobby-team-builder/champ-select/v1/crowd-favorte-champion-list",
            )
            .await?,
        )?;
        ids.retain(|id| favorites.contains(id));
    }
    let subset: Vec<i32> = if session["allowSubsetChampionPicks"] == true {
        serde_json::from_value(
            lcu(
                state,
                "/lol-lobby-team-builder/champ-select/v1/subset-champion-list",
            )
            .await?,
        )?
    } else {
        vec![]
    };
    let available = live_candidates(
        &session,
        mode,
        &preferences,
        &ids,
        &subset,
        profile.is_some_and(|p| p.bench),
    );
    if effective.auto_pick_enabled
        && !(mode == "arena" && profile.is_some_and(|p| p.arena_pick == "manual"))
    {
        result.intent.candidates =
            draft::allowed_candidates(&result.intent.preferences.pick, &available);
    }
    if matches!(mode, "classic" | "arena")
        && effective.auto_ban_enabled
        && !result.intent.preferences.ban.is_empty()
    {
        let bannable: Vec<i32> = serde_json::from_value(
            lcu(state, "/lol-champ-select/v1/bannable-champion-ids").await?,
        )?;
        result.intent.ban_candidates =
            draft::allowed_ban_candidates(&result.intent.preferences.ban, &bannable);
    }
    let fresh_flow = lcu(state, "/lol-gameflow/v1/session").await?;
    let fresh = lcu(state, "/lol-champ-select/v1/session").await?;
    let same_player = policy::player(&fresh).is_some_and(|p| {
        p["puuid"] == me["puuid"] && p["summonerId"] == me["summonerId"] && policy::role(p) == role
    });
    let game_id = session["gameId"].as_i64().filter(|id| *id > 0);
    if game_id.is_none()
        || fresh["gameId"] != session["gameId"]
        || (mode == "classic"
            && draft::classic_pick_turn(&session) != draft::classic_pick_turn(&fresh))
        || !same_player
        || policy::mode(&fresh_flow) != Some(mode)
        || started.elapsed() >= Duration::from_secs(3)
        || (matches!(mode, "aram" | "mayhem")
            && !policy::bench_session_is_current(&session, &fresh, started.elapsed()))
    {
        result.intent.candidates.clear();
        result.intent.ban_candidates.clear();
        return Ok(result);
    }
    if serde_json::to_value(state.auto_accept.get_settings().await)?
        != serde_json::to_value(&settings)?
        || state
            .settings
            .load_setting::<SavedPreferences>(PREFERENCES, BTreeMap::new())
            != saved
        || available
            != live_candidates(
                &fresh,
                mode,
                &preferences,
                &ids,
                &subset,
                profile.is_some_and(|p| p.bench),
            )
    {
        return Err(AppError::Custom(
            "Пики изменились; повторите публикацию".into(),
        ));
    }
    if mode == "classic" {
        let turn = draft::classic_pick_turn(&fresh);
        result.intent.pick_context = draft::classic_pick_context(&fresh);
        result.intent.pick_order = turn.map(|t| t.0);
        result.intent.pick_active = Some(turn.is_some_and(|t| t.1));
        result.intent.pick_role = Some(policy::role(policy::player(&fresh).unwrap()).into());
    }
    result.intent.session_key = game_id.unwrap().to_string();
    Ok(result)
}

async fn catalog_names(state: &AppState) -> Result<BTreeMap<i32, String>, AppError> {
    let mut names = BTreeMap::new();
    for name in state.data_dragon.get_champions().await?.keys() {
        if let Some(info) = state.data_dragon.get_champion_info(name).await {
            if let Ok(id) = info.id.parse::<i32>() {
                names.insert(id, name.clone());
            }
        }
    }
    Ok(names)
}

async fn palette_names(state: &AppState, result: &mut DraftLocal) {
    if result.palette_preferences.pick.is_empty() && result.palette_preferences.ban.is_empty() {
        return;
    }
    if let Ok(names) = catalog_names(state).await {
        for preference in result
            .palette_preferences
            .pick
            .iter()
            .chain(&result.palette_preferences.ban)
        {
            if let Some(name) = names.get(&preference.champion_id) {
                result
                    .champion_names
                    .insert(preference.champion_id, name.clone());
            }
        }
    }
}

async fn view(state: &AppState, mode: &str, role: &str) -> DraftLocal {
    let mut result = match local(state).await {
        Ok(live) if live.intent.mode == mode && (mode != "classic" || live.role == role) => live,
        _ => configured(
            &state.auto_accept.get_settings().await,
            &state.settings.load_setting(PREFERENCES, BTreeMap::new()),
            mode,
            role,
        ),
    };
    palette_names(state, &mut result).await;
    result
}

fn valid_context(mode: &str, role: &str) -> bool {
    matches!(mode, "aram" | "mayhem" | "arena")
        || mode == "classic" && matches!(role, "top" | "jungle" | "middle" | "bottom" | "utility")
}

#[tauri::command]
pub async fn draft_local(
    mode: Option<String>,
    role: Option<String>,
    state: State<'_, AppState>,
) -> Result<DraftLocal, AppError> {
    if mode.is_none() {
        let mut result = local(&state).await?;
        palette_names(&state, &mut result).await;
        return Ok(result);
    }
    let mode = mode.unwrap();
    let role = role.unwrap_or_default();
    if !valid_context(&mode, &role) {
        return Err(AppError::Custom("Неизвестный профиль".into()));
    }
    let mut result = view(&state, &mode, &role).await;
    result.coordination_status = if !result.auto_pick_enabled && !result.auto_ban_enabled {
        "disabled"
    } else if !(result.auto_pick_enabled
        && result
            .intent
            .preferences
            .pick
            .iter()
            .any(|p| p.state != PreferenceState::Exclude)
        || result.auto_ban_enabled
            && result
                .intent
                .preferences
                .ban
                .iter()
                .any(|p| p.state != PreferenceState::Exclude))
    {
        "empty"
    } else {
        match lcu(&state, "/lol-lobby/v2/lobby").await {
            Ok(lobby) if lobby["members"].as_array().is_some_and(|ms| ms.len() >= 2) => "active",
            Ok(_) => "solo",
            Err(_) if !result.intent.session_key.is_empty() => "waiting",
            Err(_) => "waiting",
        }
    }
    .into();
    if result.coordination_status == "solo" {
        draft::set_group_active(false, &state.settings);
    }
    Ok(result)
}

pub async fn automation_summary(state: &AppState, queue: i64, role: &str) -> Option<Value> {
    let mode = match queue {
        400 | 420 | 430 | 440 | 480 | 490 => "classic",
        450 => "aram",
        2400 => "mayhem",
        1700 | 1710 | 1750 => "arena",
        _ => return None,
    };
    let settings = state.auto_accept.get_settings().await;
    let saved = state.settings.load_setting(PREFERENCES, BTreeMap::new());
    let current = if let Ok(live) = local(state).await {
        if live.intent.mode == mode {
            live
        } else {
            configured(&settings, &saved, mode, role)
        }
    } else {
        configured(&settings, &saved, mode, role)
    };
    Some(
        serde_json::json!({"mode":mode,"autoPick":current.auto_pick_enabled && state.settings.load_setting(draft::OPT_IN,true),"autoBan":current.auto_ban_enabled && state.settings.load_setting(draft::OPT_IN,true),"preferences":current.intent.preferences}),
    )
}

/// Publish the live configured pool; changes revoke previous local permission first.
pub async fn refresh_intent(
    state: &AppState,
    own: &Value,
) -> Result<Option<DraftIntent>, AppError> {
    if !state.settings.load_setting(draft::OPT_IN, true) {
        draft::unavailable();
        return Err(AppError::Custom("Согласование выключено".into()));
    }
    let current = match local(state).await {
        Ok(v) => v,
        Err(e) => {
            draft::unavailable();
            return Err(e);
        }
    };
    if current.intent.session_key.is_empty() {
        draft::unavailable();
        return Err(AppError::Custom("Нет актуального выбора чемпионов".into()));
    }
    if serde_json::to_value(&current.intent)? == own["draft"] {
        return Ok(None);
    }
    draft::set_enabled(true);
    Ok(Some(current.intent))
}

#[tauri::command]
pub async fn draft_share_current(state: State<'_, AppState>) -> Result<DraftIntent, AppError> {
    if !state.settings.load_setting(draft::SETTING, false) {
        return Err(AppError::Custom("Сначала включите согласование".into()));
    }
    let current = local(&state).await?;
    if current.intent.session_key.is_empty() {
        return Err(AppError::Custom("Нет общего выбора чемпионов".into()));
    }
    draft::set_enabled(true);
    state.settings.save_setting(SHARE, &true)?;
    Ok(current.intent)
}

fn save_preference(
    entry: &mut DraftPreferences,
    champion_id: i32,
    kind: &str,
    state: PreferenceState,
) -> Result<(), AppError> {
    let count = entry.pick.len() + entry.ban.len();
    let changed = if kind == "pick" {
        &mut entry.pick
    } else {
        &mut entry.ban
    };
    if let Some(previous) = changed.iter_mut().find(|p| p.champion_id == champion_id) {
        previous.state = state;
    } else {
        if count >= 1000 {
            return Err(AppError::Custom(
                "Сохранено слишком много предпочтений".into(),
            ));
        }
        changed.push(ChampionPreference { champion_id, state });
    }
    Ok(())
}

#[tauri::command]
pub async fn draft_set_preference(
    champion_id: i32,
    kind: String,
    state: PreferenceState,
    mode: String,
    role: String,
    session_key: Option<String>,
    app: State<'_, AppState>,
) -> Result<DraftLocal, AppError> {
    let _edit = EDIT
        .try_lock()
        .map_err(|_| AppError::Custom("Предпочтение ещё сохраняется".into()))?;
    if !valid_context(&mode, &role)
        || !matches!(kind.as_str(), "pick" | "ban")
        || !(1..=10000).contains(&champion_id)
    {
        return Err(AppError::Custom("Некорректное предпочтение".into()));
    }
    if !catalog_names(&app).await?.contains_key(&champion_id) {
        return Err(AppError::Custom("Неизвестный чемпион".into()));
    }
    let mut saved: SavedPreferences = app.settings.load_setting(PREFERENCES, BTreeMap::new());
    if let Some(expected) = session_key.filter(|s| !s.is_empty()) {
        let live = local(&app).await?;
        if live.intent.session_key != expected
            || live.intent.mode != mode
            || (mode == "classic" && live.role != role)
        {
            return Err(AppError::Custom("Выбор чемпионов изменился".into()));
        }
    }
    let entry = saved.entry(profile_key(&mode, &role)).or_default();
    save_preference(entry, champion_id, &kind, state)?;
    app.settings.save_setting(PREFERENCES, &saved)?;
    Ok(view(&app, &mode, &role).await)
}

#[tauri::command]
pub fn draft_enabled(state: State<AppState>) -> bool {
    state.settings.load_setting(draft::OPT_IN, true)
}
#[tauri::command]
pub fn draft_set_enabled(enabled: bool, state: State<AppState>) -> Result<(), String> {
    state
        .settings
        .save_setting(draft::OPT_IN, &enabled)
        .map_err(|e| e.to_string())?;
    draft::set_enabled(enabled);
    if !enabled {
        draft::set_group_active(false, &state.settings);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::automation::AutomationProfile;
    use serde_json::json;
    #[test]
    fn all_three_bans_are_published_without_legacy_states() {
        let mut settings = AutomationSettings::default();
        settings.profiles.insert(
            "arena".into(),
            AutomationProfile {
                bans: vec!["Lux".into(), "Annie".into(), "Olaf".into()],
                ban_ids: vec![99, 1, 2],
                ban: "Lux".into(),
                ban_id: Some(99),
                auto_ban: true,
                ..Default::default()
            },
        );
        let saved = BTreeMap::from([(
            "arena".into(),
            DraftPreferences {
                pick: vec![],
                ban: vec![ChampionPreference {
                    champion_id: 99,
                    state: PreferenceState::Exclude,
                }],
            },
        )]);
        let result = configured(&settings, &saved, "arena", "");
        assert_eq!(
            result
                .intent
                .preferences
                .ban
                .iter()
                .map(|p| p.champion_id)
                .collect::<Vec<_>>(),
            vec![99, 1, 2]
        );
        assert_eq!(
            result.intent.preferences.ban[0].state,
            PreferenceState::Yield
        );
        assert_eq!(result.champion_names[&2], "Olaf");
        assert_eq!(
            draft::allowed_ban_candidates(&result.intent.preferences.ban, &[99, 1, 2]),
            vec![99, 1, 2]
        );
        assert!(result.palette_preferences.ban.is_empty());
        assert!(result.marked_preferences["ban"].is_empty());
    }

    #[test]
    fn own_pick_is_protected_regardless_of_saved_pick_or_ban_state() {
        for state in [
            PreferenceState::Must,
            PreferenceState::Yield,
            PreferenceState::Exclude,
        ] {
            let mut settings = AutomationSettings::default();
            settings.profiles.insert(
                "classic:top".into(),
                AutomationProfile {
                    picks: vec!["Lux".into()],
                    pick_ids: vec![99],
                    bans: vec!["Lux".into(), "Annie".into(), "Olaf".into()],
                    ban_ids: vec![99, 1, 2],
                    ..Default::default()
                },
            );
            let saved = BTreeMap::from([(
                "classic:top".into(),
                DraftPreferences {
                    pick: vec![ChampionPreference {
                        champion_id: 99,
                        state,
                    }],
                    ban: vec![
                        ChampionPreference {
                            champion_id: 1,
                            state: PreferenceState::Must,
                        },
                        ChampionPreference {
                            champion_id: 2,
                            state: PreferenceState::Exclude,
                        },
                    ],
                },
            )]);
            let result = configured(&settings, &saved, "classic", "top");
            assert_eq!(result.intent.preferences.pick[0].state, state);
            assert_eq!(
                draft::allowed_ban_candidates(&result.intent.preferences.ban, &[99, 1, 2]),
                vec![1, 2]
            );
            assert_eq!(
                draft::allowed_ban_candidates(&result.intent.preferences.ban, &[99, 2]),
                vec![2]
            );
        }
    }

    #[test]
    fn classic_availability_excludes_ally_picks_and_bans() {
        let session = json!({"localPlayerCellId":0,"myTeam":[{"cellId":0,"championId":0},{"cellId":1,"championId":2}],"actions":[[{"type":"ban","completed":true,"championId":3}]]});
        assert_eq!(
            live_candidates(
                &session,
                "classic",
                &[4, 1, 2, 3, 5],
                &[1, 2, 3, 4],
                &[],
                false
            ),
            vec![4, 1]
        );
    }
    #[test]
    fn arena_candidates_allow_other_subteam_duplicate_but_not_own_ally() {
        let session = json!({"localPlayerCellId":0,"myTeam":[
            {"cellId":0,"subteamId":1},{"cellId":1,"subteamId":1,"championId":2},
            {"cellId":2,"subteamId":2,"championId":3}],"actions":[]});
        assert_eq!(
            live_candidates(&session, "arena", &[2, 3], &[2, 3], &[], false),
            vec![3]
        );
    }

    #[test]
    fn random_availability_uses_shared_phase_not_elapsed_phase_guess() {
        let mut session = json!({"gameId":123,"localPlayerCellId":0,"myTeam":[{"cellId":0,"championId":1}],"benchEnabled":true,"benchChampions":[{"championId":2}],"timer":{"phase":"FINALIZATION","totalTimeInPhase":60000,"adjustedTimeLeftInPhase":59000}});
        assert_eq!(
            live_candidates(&session, "aram", &[2, 1, 3], &[1, 2, 3], &[], true),
            vec![2, 1]
        );
        session["timer"]["adjustedTimeLeftInPhase"] = json!(56000);
        assert_eq!(
            live_candidates(&session, "mayhem", &[2, 1, 3], &[1, 2, 3], &[], true),
            vec![2, 1]
        );
    }
    #[test]
    fn setup_pools_survive_no_session_and_overrides_are_mode_role_and_pool_scoped() {
        let mut settings = AutomationSettings::default();
        for key in ["classic:top", "classic:jungle", "aram", "mayhem"] {
            settings.profiles.insert(
                key.into(),
                AutomationProfile {
                    picks: vec!["One".into(), "Two".into()],
                    pick_ids: vec![1, 2],
                    ban_id: Some(3),
                    ..Default::default()
                },
            );
        }
        let saved = BTreeMap::from([(
            "classic:top".into(),
            DraftPreferences {
                pick: vec![ChampionPreference {
                    champion_id: 1,
                    state: PreferenceState::Exclude,
                }],
                ban: vec![],
            },
        )]);
        let top = configured(&settings, &saved, "classic", "top");
        assert_eq!(top.marked_preferences["pick"], vec![1]);
        assert!(
            configured(&settings, &saved, "classic", "jungle").marked_preferences["pick"]
                .is_empty()
        );
        assert!(top.intent.session_key.is_empty());
        assert!(top.intent.candidates.is_empty());
        assert_eq!(top.intent.preferences.pick.len(), 2);
        assert_eq!(
            top.intent.preferences.pick[0].state,
            PreferenceState::Exclude
        );
        assert_eq!(
            configured(&settings, &saved, "classic", "jungle")
                .intent
                .preferences
                .pick[0]
                .state,
            PreferenceState::Yield
        );
        for mode in ["aram", "mayhem"] {
            let p = configured(&settings, &saved, mode, "");
            assert!(p.intent.preferences.ban.is_empty());
            assert_eq!(p.intent.preferences.pick[0].state, PreferenceState::Yield);
        }
    }
}

#[cfg(test)]
mod palette_tests {
    use super::*;
    use crate::models::automation::AutomationProfile;

    #[test]
    fn off_pool_overrides_survive_edits_without_changing_engine_pool_or_toggles() {
        let mut settings = AutomationSettings::default();
        settings.profiles.insert(
            "classic:top".into(),
            AutomationProfile {
                picks: vec!["One".into()],
                pick_ids: vec![1],
                ..Default::default()
            },
        );
        let before = configured(&settings, &BTreeMap::new(), "classic", "top");
        let mut entry = DraftPreferences::default();
        save_preference(&mut entry, 2, "pick", PreferenceState::Must).unwrap();
        save_preference(&mut entry, 3, "ban", PreferenceState::Exclude).unwrap();
        save_preference(&mut entry, 4, "pick", PreferenceState::Yield).unwrap();
        let saved = BTreeMap::from([("classic:top".into(), entry.clone())]);
        let after = configured(&settings, &saved, "classic", "top");
        assert_eq!(after.palette_preferences.pick, entry.pick);
        assert!(after.palette_preferences.ban.is_empty());
        assert_eq!(after.intent.preferences, before.intent.preferences);
        assert_eq!(after.auto_pick_enabled, before.auto_pick_enabled);
        assert_eq!(after.auto_ban_enabled, before.auto_ban_enabled);
        assert!(configured(&settings, &saved, "classic", "jungle")
            .palette_preferences
            .pick
            .is_empty());
    }

    #[test]
    fn palette_keeps_more_than_thirty_entries_and_caps_new_overrides_at_one_thousand() {
        let mut entry = DraftPreferences::default();
        for id in 1..=1000 {
            save_preference(&mut entry, id, "pick", PreferenceState::Yield).unwrap();
        }
        assert_eq!(entry.pick.len(), 1000);
        assert!(save_preference(&mut entry, 1001, "ban", PreferenceState::Exclude).is_err());
        save_preference(&mut entry, 1, "pick", PreferenceState::Exclude).unwrap();
        assert_eq!(entry.pick.len(), 1000);
        assert_eq!(entry.pick[0].state, PreferenceState::Exclude);
        assert!(entry.ban.is_empty());
    }
}
