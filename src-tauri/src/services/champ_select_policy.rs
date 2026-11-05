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
    let remaining = session["timer"]["adjustedTimeLeftInPhase"].as_f64()?;
    if !remaining.is_finite() || remaining <= 250.0 {
        return None;
    }
    let own_cards =
        session["allowSubsetChampionPicks"] == true && session["timer"]["phase"] == "BAN_PICK";
    if !own_cards && (!shared || session["timer"]["phase"] != "FINALIZATION") {
        return None;
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

pub fn bench_session_is_current(
    previous: &Value,
    fresh: &Value,
    elapsed: std::time::Duration,
) -> bool {
    let Some(game_id) = previous["gameId"].as_i64().filter(|id| *id > 0) else {
        return false;
    };
    if fresh["gameId"].as_i64() != Some(game_id)
        || fresh["localPlayerCellId"] != previous["localPlayerCellId"]
        || fresh["timer"]["phase"] != previous["timer"]["phase"]
    {
        return false;
    }
    let (Some(before), Some(now)) = (player(previous), player(fresh)) else {
        return false;
    };
    if before["championId"] != now["championId"]
        || before["championPickIntent"] != now["championPickIntent"]
        || previous["allowSubsetChampionPicks"] != fresh["allowSubsetChampionPicks"]
    {
        return false;
    }
    // Reserve a short round-trip margin; a late response must not trigger an expired swap.
    fresh["timer"]["adjustedTimeLeftInPhase"]
        .as_f64()
        .is_some_and(|ms| ms > elapsed.as_secs_f64() * 1000.0 + 250.0)
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

/// Arena may expose every subteam in myTeam. Missing identity never permits duplicates.
pub fn same_pick_team(session: &Value, other: &Value) -> bool {
    let Some(me) = player(session) else {
        return true;
    };
    for field in ["subteamId", "team"] {
        if let (Some(own), Some(theirs)) = (me[field].as_i64(), other[field].as_i64()) {
            if own >= 0 && theirs >= 0 {
                return own == theirs;
            }
        }
    }
    true
}

/// A shared team number alone does not identify an Arena subteam without reported size.
pub fn arena_team_known(session: &Value, team_size: Option<u64>) -> bool {
    let Some(me) = player(session) else {
        return false;
    };
    let Some(team) = session["myTeam"].as_array() else {
        return false;
    };
    let Some(size) = team_size.filter(|n| *n > 0) else {
        return false;
    };
    if team.len() as u64 == size {
        return true;
    }
    ["subteamId", "team"].iter().any(|field| {
        let Some(own) = me[*field].as_i64().filter(|id| *id >= 0) else {
            return false;
        };
        team.iter()
            .all(|p| p[*field].as_i64().is_some_and(|id| id >= 0))
            && team
                .iter()
                .filter(|p| p[*field].as_i64() == Some(own))
                .count() as u64
                == size
    })
}

pub fn bans_finished(session: &Value) -> bool {
    session["actions"].as_array().is_some_and(|groups| {
        groups
            .iter()
            .filter_map(Value::as_array)
            .flatten()
            .all(|a| a["type"] != "ban" || a["completed"] == true)
    })
}

/// One personal choice per stage, bounded shared swaps, and no undo after manual mutation.
#[derive(Default)]
pub struct BenchProgress {
    game: i64,
    cell: i64,
    phase: String,
    observed: Option<i32>,
    observed_intent: Option<i32>,
    expected: Option<i32>,
    manual: bool,
    personal_attempted: bool,
    personal_choice: Option<Option<i32>>,
    attempts: u8,
    cards: std::collections::BTreeMap<i32, std::time::Instant>,
}
impl BenchProgress {
    pub fn observe(&mut self, session: &Value, now: std::time::Instant) {
        let game = session["gameId"].as_i64().unwrap_or(0);
        let cell = session["localPlayerCellId"].as_i64().unwrap_or(-1);
        if self.game != game || self.cell != cell {
            *self = Self {
                game,
                cell,
                ..Self::default()
            };
        }
        let phase = session["timer"]["phase"].as_str().unwrap_or("");
        if self.phase != phase {
            if self.phase == "BAN_PICK" && phase == "FINALIZATION" {
                self.manual = false;
                self.observed = None;
                self.observed_intent = None;
            }
            self.phase = phase.into();
            self.personal_attempted = false;
            self.personal_choice = None;
            self.attempts = 0;
        }
        let expected = self.expected;
        if let Some(id) = player(session)
            .and_then(|p| p["championId"].as_i64())
            .map(|id| id as i32)
        {
            if self.observed.is_some_and(|old| old > 0 && old != id) {
                if self.expected == Some(id) {
                    self.expected = None;
                } else {
                    self.manual = true;
                }
            }
            self.observed = Some(id);
        }
        if let Some(id) = player(session)
            .and_then(|p| p["championPickIntent"].as_i64())
            .map(|id| id as i32)
        {
            if id > 0 && self.observed_intent.is_some_and(|old| old != id) && expected != Some(id) {
                self.manual = true;
            }
            self.observed_intent = Some(id);
        }
        let ids: Vec<i32> = session["benchChampions"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|c| c["championId"].as_i64().map(|id| id as i32))
            .collect();
        self.cards.retain(|id, _| ids.contains(id));
        for id in ids {
            self.cards.entry(id).or_insert(now);
        }
    }
    pub fn permits(&self, session: &Value, id: i32, now: std::time::Instant) -> bool {
        if self.game <= 0 || self.cell < 0 || self.manual || self.attempts >= 3 {
            return false;
        }
        if session["timer"]["phase"] == "BAN_PICK" {
            return !self.personal_attempted;
        }
        // Client ally-swap cooldown is 3s per newly observed card, not per finalization.
        self.cards.get(&id).is_some_and(|at| {
            now.saturating_duration_since(*at) >= std::time::Duration::from_secs(3)
        })
    }
    pub fn personal_choice(&mut self, preferred: &[i32], eligible: &[i32]) -> Option<i32> {
        if self.manual || self.personal_attempted {
            return None;
        }
        let choice = *self.personal_choice.get_or_insert_with(|| {
            preferred
                .iter()
                .copied()
                .find(|id| eligible.contains(id))
                .or_else(|| {
                    if eligible.is_empty() {
                        None
                    } else {
                        Some(eligible[(rand::random::<u64>() % eligible.len() as u64) as usize])
                    }
                })
        });
        choice.filter(|id| eligible.contains(id))
    }
    pub fn personal_matches(&self, id: i32, eligible: &[i32]) -> bool {
        !self.manual && self.personal_choice == Some(Some(id)) && eligible.contains(&id)
    }
    pub fn attempt(&mut self, id: i32, personal: bool) {
        self.attempts += 1;
        self.personal_attempted |= personal;
        self.expected = Some(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::automation::{AutomationProfile, AutomationSettings};
    use serde_json::json;
    #[test]
    fn bench_revalidation_rejects_new_games_changed_champions_and_expired_timers() {
        let base = json!({"gameId":10,"localPlayerCellId":1,"myTeam":[{"cellId":1,"championId":3}],"timer":{"phase":"FINALIZATION","adjustedTimeLeftInPhase":4000.5}});
        let delay = std::time::Duration::from_millis(200);
        assert!(bench_session_is_current(&base, &base, delay));
        for (pointer, value) in [
            ("/gameId", json!(11)),
            ("/localPlayerCellId", json!(2)),
            ("/myTeam/0/championId", json!(2)),
            ("/timer/phase", json!("BAN_PICK")),
            ("/timer/adjustedTimeLeftInPhase", json!(450)),
            ("/timer/adjustedTimeLeftInPhase", json!(0)),
            ("/timer/adjustedTimeLeftInPhase", json!(null)),
        ] {
            let mut fresh = base.clone();
            *fresh.pointer_mut(pointer).unwrap() = value;
            assert!(!bench_session_is_current(&base, &fresh, delay), "{pointer}");
        }
        assert!(!bench_session_is_current(
            &base,
            &base,
            std::time::Duration::from_secs(5)
        ));
    }

    #[test]
    fn finalization_is_not_a_three_second_unlock_timer_and_expiry_stops_swaps() {
        let mut session = json!({"localPlayerCellId":1,"myTeam":[{"cellId":1,"championId":3}],"benchEnabled":true,"benchChampions":[{"championId":1}],"timer":{"phase":"FINALIZATION","totalTimeInPhase":45000.5,"adjustedTimeLeftInPhase":42000.6}});
        assert_eq!(bench_pick(&session, &[1, 3], &[1], &[], true), Some(1));
        session["timer"]["adjustedTimeLeftInPhase"] = json!(42000.5);
        assert_eq!(bench_pick(&session, &[1, 3], &[1], &[], true), Some(1));
        session["timer"]["adjustedTimeLeftInPhase"] = json!(0);
        assert_eq!(bench_pick(&session, &[1, 3], &[1], &[], true), None);
    }

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
        assert_eq!(
            bench_pick(&session, &[1, 2, 3], &[1, 2], &[], true),
            Some(1)
        );
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
        let session = json!({"localPlayerCellId":1,"myTeam":[{"cellId":1,"championId":3}],"benchEnabled":true,"allowSubsetChampionPicks":true,"benchChampions":[{"championId":1},{"championId":2}],"timer":{"phase":"BAN_PICK","adjustedTimeLeftInPhase":10000}});
        assert_eq!(
            bench_pick(&session, &[1, 2, 3], &[1, 2], &[2, 3], false),
            Some(2)
        );
        assert_eq!(bench_pick(&session, &[1, 2, 3], &[1, 2], &[], true), None);
        assert_eq!(bench_pick(&session, &[1, 2, 3], &[1, 2], &[3], false), None);
    }
}

/// Keep the fallback until real availability changes; unknown safety never rerolls it.
pub struct BanFallback {
    seed: u64,
    attempted: Option<((i64, i64, i64), Option<i32>)>,
}

impl Default for BanFallback {
    fn default() -> Self {
        Self {
            seed: rand::random(),
            attempted: None,
        }
    }
}

impl BanFallback {
    pub fn choose(
        &mut self,
        key: (i64, i64, i64),
        configured: &[i32],
        available: &[i32],
        eligible: &[i32],
    ) -> Option<i32> {
        use std::hash::{Hash, Hasher};
        if let Some(id) = configured.iter().find(|id| eligible.contains(id)) {
            return Some(*id);
        }
        if eligible.is_empty() {
            return None;
        }
        let invalidated = self.attempted.as_ref().is_some_and(|(previous, choice)| {
            *previous == key && choice.is_some_and(|id| !available.contains(&id))
        });
        if self.attempted.as_ref().map(|(previous, _)| *previous) != Some(key) || invalidated {
            let choice = eligible.iter().copied().min_by_key(|id| {
                let mut hash = std::collections::hash_map::DefaultHasher::new();
                (self.seed, key, id).hash(&mut hash);
                (hash.finish(), *id)
            });
            self.attempted = Some((key, choice));
        }
        self.attempted
            .as_ref()
            .and_then(|(_, choice)| *choice)
            .filter(|id| eligible.contains(id))
    }
}

#[cfg(test)]
mod ban_fallback_tests {
    use super::BanFallback;

    #[test]
    fn second_and_third_configured_bans_precede_random() {
        let mut fallback = BanFallback::default();
        assert_eq!(
            fallback.choose((1, 2, 0), &[99, 3, 4], &[3, 4, 5], &[3, 4, 5]),
            Some(3)
        );
        assert_eq!(
            fallback.choose((1, 2, 0), &[99, 3, 4], &[4, 5], &[4, 5]),
            Some(4)
        );
    }

    #[test]
    fn random_is_stable_until_real_availability_invalidates_it() {
        let mut fallback = BanFallback::default();
        let selected = fallback.choose((1, 2, 0), &[99], &[4, 5], &[4, 5]).unwrap();
        assert_eq!(
            fallback.choose((1, 2, 0), &[99], &[5, 4, 6], &[5, 4, 6]),
            Some(selected)
        );
        let remaining = if selected == 4 { 5 } else { 4 };
        // Unknown or changed cooperation safety must not replace the still-available choice.
        assert_eq!(fallback.choose((1, 2, 0), &[99], &[4, 5], &[]), None);
        assert_eq!(
            fallback.choose((1, 2, 0), &[99], &[4, 5], &[remaining]),
            None
        );
        // Actual unavailability allows one replacement, then that replacement stays fixed.
        assert_eq!(
            fallback.choose((1, 2, 0), &[99], &[remaining], &[remaining]),
            Some(remaining)
        );
        assert_eq!(
            fallback.choose((1, 2, 0), &[99], &[4, 5, 6], &[4, 5, 6]),
            Some(remaining)
        );
    }

    #[test]
    fn unknown_safety_does_not_clear_an_invalidated_choice_or_seed_a_new_one() {
        let mut fallback = BanFallback::default();
        assert_eq!(fallback.choose((1, 2, 0), &[99], &[4], &[]), None);
        assert_eq!(fallback.choose((1, 2, 0), &[99], &[4], &[4]), Some(4));
        assert_eq!(fallback.choose((1, 2, 0), &[99], &[5], &[]), None);
        assert_eq!(fallback.choose((1, 2, 0), &[99], &[4, 5], &[4, 5]), Some(4));
        assert_eq!(fallback.choose((1, 2, 0), &[99], &[], &[]), None);
        assert_eq!(fallback.choose((1, 2, 0), &[99], &[5], &[5]), Some(5));
    }
}

#[cfg(test)]
mod mode_automation_tests {
    use super::*;
    use serde_json::json;
    use std::time::{Duration, Instant};

    fn cards() -> Value {
        json!({"gameId":42,"localPlayerCellId":1,"myTeam":[{"cellId":1,"championId":3,"championPickIntent":0}],
            "allowSubsetChampionPicks":true,"benchEnabled":true,"benchChampions":[{"championId":1},{"championId":2}],
            "timer":{"phase":"BAN_PICK","adjustedTimeLeftInPhase":12000}})
    }

    #[test]
    fn two_or_three_personal_cards_never_include_a_teammates_private_card() {
        let session = cards();
        for subset in [&[2, 3][..], &[2, 3, 4][..]] {
            assert_eq!(
                bench_pick(&session, &[1, 2, 3], &[1, 2, 3, 4], subset, true),
                Some(2)
            );
            assert_eq!(
                bench_pick(&session, &[1, 3], &[1, 2, 3, 4], subset, true),
                None
            );
        }
        let mut unknown = session.clone();
        unknown["timer"]["adjustedTimeLeftInPhase"] = Value::Null;
        assert_eq!(bench_pick(&unknown, &[2], &[2], &[2, 3], true), None);
        unknown["timer"]["adjustedTimeLeftInPhase"] = json!(10000);
        unknown["timer"]["phase"] = json!("UNKNOWN");
        assert_eq!(bench_pick(&unknown, &[2], &[2], &[2, 3], true), None);
    }

    #[test]
    fn random_is_one_eligible_choice_and_does_not_reroll_on_availability_changes() {
        let mut progress = BenchProgress::default();
        progress.observe(&cards(), Instant::now());
        let id = progress.personal_choice(&[], &[1, 2, 3]).unwrap();
        assert_eq!(progress.personal_choice(&[], &[3, 2, 1]), Some(id));
        let others: Vec<_> = [1, 2, 3].into_iter().filter(|other| *other != id).collect();
        assert_eq!(progress.personal_choice(&[], &others), None);
        assert_eq!(progress.personal_choice(&[], &[id]), Some(id));
        progress.attempt(id, true);
        assert_eq!(progress.personal_choice(&[], &[1, 2, 3]), None);
    }

    #[test]
    fn personal_prefers_eligible_priority_and_all_red_means_manual() {
        let mut progress = BenchProgress::default();
        progress.observe(&cards(), Instant::now());
        assert_eq!(progress.personal_choice(&[9, 2, 1], &[1, 2, 3]), Some(2));
        let mut red = BenchProgress::default();
        red.observe(&cards(), Instant::now());
        assert_eq!(red.personal_choice(&[], &[]), None);
        assert_eq!(red.personal_choice(&[1], &[1]), None);
    }

    #[test]
    fn manual_card_and_hover_mutations_are_never_undone() {
        for hover in [false, true] {
            let mut session = cards();
            let now = Instant::now();
            let mut progress = BenchProgress::default();
            progress.observe(&session, now);
            session["myTeam"][0][if hover {
                "championPickIntent"
            } else {
                "championId"
            }] = json!(2);
            progress.observe(&session, now);
            assert!(!progress.permits(&session, 1, now + Duration::from_secs(4)));
            assert_eq!(progress.personal_choice(&[1], &[1, 2]), None);
            session["timer"]["phase"] = json!("FINALIZATION");
            progress.observe(&session, now);
            assert!(progress.permits(&session, 1, now + Duration::from_secs(4)));
        }
    }

    #[test]
    fn own_mutation_is_not_manual_and_shared_card_cooldown_tracks_each_arrival() {
        let mut session = cards();
        let now = Instant::now();
        let mut progress = BenchProgress::default();
        progress.observe(&session, now);
        progress.attempt(2, true);
        session["myTeam"][0]["championId"] = json!(2);
        progress.observe(&session, now);
        session["timer"]["phase"] = json!("FINALIZATION");
        progress.observe(&session, now + Duration::from_secs(1));
        assert!(!progress.permits(&session, 1, now + Duration::from_secs(2)));
        assert!(progress.permits(&session, 1, now + Duration::from_secs(3)));
        session["benchChampions"][0]["championId"] = json!(4);
        progress.observe(&session, now + Duration::from_secs(4));
        assert!(!progress.permits(&session, 4, now + Duration::from_secs(6)));
        assert!(progress.permits(&session, 4, now + Duration::from_secs(7)));
        for _ in 0..3 {
            progress.attempt(4, false);
        }
        assert!(!progress.permits(&session, 4, now + Duration::from_secs(8)));
        session["gameId"] = json!(43);
        progress.observe(&session, now + Duration::from_secs(9));
        assert!(progress.permits(&session, 4, now + Duration::from_secs(12)));
    }

    #[test]
    fn shared_bench_only_upgrades_and_never_downgrades_a_current_preference() {
        let mut session = cards();
        session["timer"]["phase"] = json!("FINALIZATION");
        assert_eq!(
            bench_pick(&session, &[1, 2, 3], &[1, 2, 3], &[], true),
            Some(1)
        );
        session["myTeam"][0]["championId"] = json!(1);
        assert_eq!(
            bench_pick(&session, &[1, 2, 3], &[1, 2, 3], &[], true),
            None
        );
        assert_eq!(bench_pick(&session, &[2], &[2], &[], false), None);
    }

    #[test]
    fn arena_exact_reported_roster_is_unambiguous_without_subteam_metadata() {
        let session = json!({"localPlayerCellId":1,"myTeam":[{"cellId":1},{"cellId":2}]});
        assert!(arena_team_known(&session, Some(2)));
        assert!(!arena_team_known(&session, Some(3)));
    }

    #[test]
    fn personal_manual_choice_does_not_disable_later_shared_bench() {
        let now = Instant::now();
        let mut session = cards();
        let mut progress = BenchProgress::default();
        session["timer"]["phase"] = json!("BAN_PICK");
        progress.observe(&session, now);
        session["myTeam"][0]["championId"] = json!(99);
        progress.observe(&session, now);
        assert!(progress.manual);
        session["timer"]["phase"] = json!("FINALIZATION");
        progress.observe(&session, now);
        assert!(!progress.manual);
        session["myTeam"][0]["championId"] = json!(98);
        progress.observe(&session, now);
        assert!(progress.manual);
    }

    #[test]
    fn arena_bans_finish_before_picks_without_assuming_a_count() {
        for count in [0, 1, 6, 12] {
            let bans: Vec<_> = (0..count)
                .map(|_| json!({"type":"ban","completed":true}))
                .collect();
            let mut session = json!({"actions":[bans]});
            assert!(bans_finished(&session));
            session["actions"][0]
                .as_array_mut()
                .unwrap()
                .push(json!({"type":"ban","completed":false,"isInProgress":false}));
            assert!(!bans_finished(&session));
        }
        assert!(!bans_finished(&json!({})));
    }

    #[test]
    fn arena_ambiguous_metadata_is_manual_for_reported_two_or_three_player_teams() {
        let mut session = cards();
        assert!(!arena_team_known(&session, Some(2)));
        session["myTeam"] = json!([{"cellId":1,"team":100},{"cellId":2,"team":100},{"cellId":3,"team":100},{"cellId":4,"team":100}]);
        assert!(!arena_team_known(&session, Some(2)));
        session["myTeam"] = json!([{"cellId":1,"subteamId":0},{"cellId":2,"subteamId":0},{"cellId":3,"subteamId":1},{"cellId":4,"subteamId":1}]);
        assert!(arena_team_known(&session, Some(2)));
        assert!(!arena_team_known(&session, Some(3)));
        assert!(!arena_team_known(&session, None));
        session["myTeam"]
            .as_array_mut()
            .unwrap()
            .push(json!({"cellId":5,"subteamId":0}));
        assert!(arena_team_known(&session, Some(3)));
    }

    #[test]
    fn native_personal_fixture_counts_one_random_mutation_and_zero_after_manual_hold() {
        let now = Instant::now();
        let mut session = cards();
        let mut progress = BenchProgress::default();
        progress.observe(&session, now);
        let chosen = progress.personal_choice(&[], &[1, 2]).unwrap();
        for _ in 0..10 {
            if progress.permits(&session, chosen, now) {
                progress.attempt(chosen, true);
            }
        }
        assert_eq!(progress.attempts, 1);
        session["myTeam"][0]["championId"] = json!(chosen);
        session["myTeam"][0]["championPickIntent"] = json!(chosen);
        progress.observe(&session, now);
        assert!(
            !progress.manual,
            "our champion and intent acknowledgement is not manual"
        );
        session["myTeam"][0]["championId"] = json!(99);
        progress.observe(&session, now);
        let before = progress.attempts;
        for _ in 0..10 {
            assert!(!progress.permits(&session, chosen, now));
        }
        assert_eq!(progress.attempts - before, 0);
    }

    #[test]
    fn traffic_lights_choose_green_then_yellow_and_random_never_red() {
        use crate::services::draft_coordination::{
            allowed_candidates, ChampionPreference, PreferenceState,
        };
        let colors = [
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
        for (physical, expected) in [
            (vec![1, 2, 3], vec![1]),
            (vec![2, 3], vec![2]),
            (vec![3], vec![]),
        ] {
            let eligible = allowed_candidates(&colors, &physical);
            assert_eq!(eligible, expected);
            let mut progress = BenchProgress::default();
            progress.observe(&cards(), Instant::now());
            assert_eq!(
                progress.personal_choice(&[], &eligible),
                expected.first().copied()
            );
        }
    }

    #[test]
    fn arena_team_identity_allows_enemy_duplicates_and_fails_closed_when_unknown() {
        let mut session = cards();
        session["myTeam"][0]["team"] = json!(100);
        session["myTeam"][0]["subteamId"] = json!(3);
        assert!(same_pick_team(&session, &json!({"team":100,"subteamId":3})));
        assert!(!same_pick_team(
            &session,
            &json!({"team":100,"subteamId":4})
        ));
        assert!(!same_pick_team(&session, &json!({"team":200})));
        assert!(same_pick_team(&session, &json!({})));
    }
}
