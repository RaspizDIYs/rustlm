use serde_json::Value;

pub fn mode(flow: &Value) -> Option<&'static str> {
    if flow["phase"].as_str() != Some("ChampSelect") {
        return None;
    }
    let queue = &flow["gameData"]["queue"];
    match (queue["gameMode"].as_str()?, queue["id"].as_i64()) {
        ("CHERRY", _) => Some("arena"),
        ("ARAM_MAYHEM", _) | ("KIWI", _) => Some("mayhem"),
        ("ARAM", Some(2400)) => Some("mayhem"),
        ("ARAM", _) => Some("aram"),
        ("CLASSIC", _) if queue["mapId"].as_i64() == Some(11) => Some("classic"),
        _ => None,
    }
}

pub fn player(session: &Value) -> Option<&Value> {
    if session["isSpectating"].as_bool() == Some(true)
        || session["skipChampionSelect"].as_bool() == Some(true)
    {
        return None;
    }
    let cell = session["localPlayerCellId"]
        .as_i64()
        .filter(|id| *id >= 0)?;
    session["myTeam"]
        .as_array()?
        .iter()
        .find(|p| p["cellId"].as_i64() == Some(cell))
}

pub fn role(player: &Value) -> &str {
    match player["assignedPosition"].as_str().unwrap_or("") {
        "top" | "TOP" => "top",
        "jungle" | "JUNGLE" => "jungle",
        "middle" | "MIDDLE" | "mid" => "middle",
        "bottom" | "BOTTOM" | "adc" => "bottom",
        "utility" | "UTILITY" | "support" => "utility",
        _ => "unknown",
    }
}

pub fn spell_allowed(mode: &str, id: i32) -> bool {
    match mode {
        "classic" => matches!(id, 1 | 3 | 4 | 6 | 7 | 11 | 12 | 14 | 21),
        "aram" => matches!(id, 1 | 3 | 4 | 6 | 7 | 13 | 14 | 21 | 32),
        "mayhem" => matches!(id, 1 | 4 | 6 | 7 | 13 | 14 | 21 | 32),
        _ => false,
    }
}

pub fn bench_pick(
    session: &Value,
    preferences: &[i32],
    pickable: &[i32],
    subset: &[i32],
    shared: bool,
) -> Option<i32> {
    if session["benchEnabled"].as_bool() != Some(true) {
        return None;
    }
    let own_cards = session["allowSubsetChampionPicks"].as_bool() == Some(true)
        && session["timer"]["phase"].as_str() == Some("BAN_PICK");
    if !own_cards {
        if !shared || session["timer"]["phase"].as_str() != Some("FINALIZATION") {
            return None;
        }
        let total = session["timer"]["totalTimeInPhase"].as_i64()?;
        let remaining = session["timer"]["adjustedTimeLeftInPhase"].as_i64()?;
        // The shared pool opens three seconds into finalization; never borrow a teammate's reserved card.
        if total <= 0 || remaining <= 0 || remaining > total || total - remaining < 3000 {
            return None;
        }
    }
    let current = player(session)?["championId"].as_i64()? as i32;
    if current <= 0 {
        return None;
    }
    let rank = preferences
        .iter()
        .position(|id| *id == current)
        .unwrap_or(preferences.len());
    preferences.iter().take(rank).copied().find(|id| {
        *id > 0
            && pickable.contains(id)
            && (!own_cards || subset.contains(id))
            && session["benchChampions"].as_array().is_some_and(|bench| {
                bench
                    .iter()
                    .any(|c| c["championId"].as_i64() == Some(*id as i64))
            })
    })
}

pub fn action_is_current(
    session: &Value,
    action_id: i64,
    game_id: i64,
    expected_role: &str,
    is_ban: bool,
    champion_id: i32,
) -> bool {
    if session["gameId"].as_i64() != Some(game_id) {
        return false;
    }
    let Some(me) = player(session) else {
        return false;
    };
    if role(me) != expected_role {
        return false;
    }
    if is_ban
        && session["myTeam"].as_array().is_some_and(|team| {
            team.iter().any(|p| {
                p["cellId"] != me["cellId"]
                    && (p["championPickIntent"].as_i64() == Some(champion_id as i64)
                        || p["championId"].as_i64() == Some(champion_id as i64))
            })
        })
    {
        return false;
    }
    session["actions"].as_array().is_some_and(|groups| {
        groups
            .iter()
            .filter_map(Value::as_array)
            .flatten()
            .any(|a| {
                a["id"].as_i64() == Some(action_id)
                    && a["actorCellId"] == me["cellId"]
                    && a["completed"].as_bool() == Some(false)
                    && a["isInProgress"].as_bool() == Some(true)
                    && a["type"].as_str() == Some(if is_ban { "ban" } else { "pick" })
            })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::automation::{AutomationProfile, AutomationSettings};
    use serde_json::json;
    #[test]
    fn queue_policy_rejects_unknown_games_and_separates_modes() {
        for (game_mode, id, expected) in [
            ("CLASSIC", 420, Some("classic")),
            ("ARAM", 450, Some("aram")),
            ("ARAM", 2400, Some("mayhem")),
            ("KIWI", 2400, Some("mayhem")),
            ("CHERRY", 1700, Some("arena")),
            ("URF", 900, None),
        ] {
            let flow = json!({"phase":"ChampSelect","gameData":{"queue":{"gameMode":game_mode,"id":id,"mapId":11}}});
            assert_eq!(mode(&flow), expected);
        }
        assert_eq!(mode(&json!({"phase":"InProgress"})), None);
    }
    #[test]
    fn roles_are_always_enforced_even_when_the_old_switch_was_disabled() {
        let mut s = AutomationSettings::default();
        s.pick_champion1_id = Some(12);
        s.use_role_profiles = false;
        assert!(!s.for_queue("classic", "jungle").auto_pick_enabled);
        s.profiles.insert(
            "classic:utility".into(),
            AutomationProfile {
                picks: vec!["Alistar".into()],
                pick_ids: vec![12],
                auto_pick: true,
                ..Default::default()
            },
        );
        assert!(!s.for_queue("classic", "jungle").auto_pick_enabled);
        assert!(!s.for_queue("classic", "unknown").auto_pick_enabled);
        assert_eq!(
            s.for_queue("classic", "utility").pick_champion1_id,
            Some(12)
        );
        assert!(!s.for_queue("aram", "").auto_pick_enabled);
    }
    #[test]
    fn arena_and_mayhem_cannot_write_runes_or_invalid_spells() {
        let mut s = AutomationSettings::default();
        for key in ["aram", "mayhem", "arena"] {
            s.profiles.insert(
                key.into(),
                AutomationProfile {
                    auto_pick: true,
                    auto_ban: true,
                    auto_spells: true,
                    auto_runes: true,
                    ..Default::default()
                },
            );
        }
        assert!(!s.for_queue("mayhem", "").auto_runes_enabled);
        assert!(!s.for_queue("arena", "").auto_spells_enabled);
        assert!(!s.for_queue("arena", "").auto_runes_enabled);
        assert!(!s.for_queue("aram", "").auto_ban_enabled);
        assert!(s.for_queue("aram", "").auto_runes_enabled);
        assert!(spell_allowed("aram", 3));
        assert!(!spell_allowed("mayhem", 3));
        for mode in ["aram", "mayhem"] {
            assert!(!spell_allowed(mode, 11));
            assert!(!spell_allowed(mode, 12));
            assert!(spell_allowed(mode, 32));
            assert!(spell_allowed(mode, 13));
        }
    }
    #[test]
    fn bench_waits_for_unlock_and_only_upgrades_to_an_available_preference() {
        let mut session = json!({"localPlayerCellId":1,"myTeam":[{"cellId":1,"championId":3}],"benchEnabled":true,"benchChampions":[{"championId":1},{"championId":2}],"timer":{"phase":"FINALIZATION","totalTimeInPhase":45000,"adjustedTimeLeftInPhase":43000}});
        assert_eq!(bench_pick(&session, &[1, 2, 3], &[1, 2], &[], true), None);
        session["timer"]["adjustedTimeLeftInPhase"] = json!(41000);
        assert_eq!(bench_pick(&session, &[1, 2, 3], &[2], &[], true), Some(2));
        assert_eq!(bench_pick(&session, &[1, 2, 3], &[], &[], true), None);
        session["myTeam"][0]["championId"] = json!(1);
        assert_eq!(bench_pick(&session, &[1, 2, 3], &[1, 2], &[], true), None);
        session["isSpectating"] = json!(true);
        assert_eq!(bench_pick(&session, &[2, 1], &[2], &[], true), None);
    }
    #[test]
    fn stale_actions_cannot_lock_after_role_swap_or_lobby_change() {
        let mut session = json!({"gameId":10,"localPlayerCellId":1,"myTeam":[{"cellId":1,"assignedPosition":"jungle"},{"cellId":2,"championPickIntent":5}],"actions":[[{"id":3,"actorCellId":1,"completed":false,"isInProgress":true,"type":"pick"}]]});
        assert!(action_is_current(&session, 3, 10, "jungle", false, 4));
        assert!(!action_is_current(&session, 3, 11, "jungle", false, 4));
        assert!(!action_is_current(&session, 3, 10, "utility", false, 4));
        session["actions"][0][0]["type"] = json!("ban");
        assert!(!action_is_current(&session, 3, 10, "jungle", true, 5));
        assert!(action_is_current(&session, 3, 10, "jungle", true, 4));
        session["actions"][0][0]["isInProgress"] = json!(false);
        assert!(!action_is_current(&session, 3, 10, "jungle", true, 4));
    }
    #[test]
    fn during_card_reveal_only_the_players_own_subset_can_be_selected() {
        let session = json!({"localPlayerCellId":1,"myTeam":[{"cellId":1,"championId":3}],"benchEnabled":true,"allowSubsetChampionPicks":true,"benchChampions":[{"championId":1},{"championId":2}],"timer":{"phase":"BAN_PICK"}});
        assert_eq!(
            bench_pick(&session, &[1, 2, 3], &[1, 2], &[2, 3], false),
            Some(2)
        );
        assert_eq!(bench_pick(&session, &[1, 2, 3], &[1, 2], &[], true), None);
        assert_eq!(bench_pick(&session, &[1, 2, 3], &[1, 2], &[3], false), None);
    }
}
