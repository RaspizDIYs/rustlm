use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiEvent {
    pub uri: String,
    pub event_type: String,
    #[serde(default)]
    pub data: Value,
}

pub fn api_event(message: &str) -> Option<ApiEvent> {
    let (kind, topic, event): (u8, String, ApiEvent) = serde_json::from_str(message).ok()?;
    (kind == 8
        && topic == "OnJsonApiEvent"
        && matches!(event.event_type.as_str(), "Create" | "Update" | "Delete"))
    .then_some(event)
}

pub fn ready_timer(data: &Value) -> Option<f64> {
    if data["state"].as_str()? != "InProgress" || data["playerResponse"].as_str()? != "None" {
        return None;
    }
    data["timer"]
        .as_f64()
        .filter(|timer| timer.is_finite() && *timer >= 0.0)
}

pub fn acceptance_confirmed(before: &Value, after: &Value, ready: &Value) -> bool {
    let game = |flow: &Value| flow["gameData"]["gameId"].as_i64().filter(|id| *id > 0);
    let original = game(before);
    let current = game(after);
    if matches!((original,current),(Some(a),Some(b)) if a!=b) {
        return false;
    }
    ready["playerResponse"] == "Accepted"
        || (before["phase"] == "ReadyCheck"
            && original.is_some()
            && original == current
            && matches!(
                after["phase"].as_str(),
                Some("ChampSelect" | "GameStart" | "InProgress")
            ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn acceptance_transition_requires_same_positive_game_and_does_not_confuse_next_match() {
        let initial = json!({"phase":"ReadyCheck","gameData":{"gameId":100}});
        for phase in ["ChampSelect", "GameStart", "InProgress"] {
            let next = json!({"phase":phase,"gameData":{"gameId":100}});
            assert!(acceptance_confirmed(&initial, &next, &Value::Null));
            let other = json!({"phase":phase,"gameData":{"gameId":101}});
            assert!(!acceptance_confirmed(
                &initial,
                &other,
                &json!({"playerResponse":"Accepted"})
            ));
        }
        for phase in ["Lobby", "Matchmaking", "ReadyCheck", "EndOfGame"] {
            assert!(!acceptance_confirmed(
                &initial,
                &json!({"phase":phase,"gameData":{"gameId":100}}),
                &Value::Null
            ));
        }
        assert!(!acceptance_confirmed(
            &Value::Null,
            &json!({"phase":"ChampSelect"}),
            &Value::Null
        ));
        assert!(acceptance_confirmed(
            &Value::Null,
            &Value::Null,
            &json!({"playerResponse":"Accepted"})
        ));
    }

    #[test]
    fn upstream_champ_select_fixtures_survive_event_parsing_and_redaction() {
        for (payload, phase) in [
            (include_str!("../../fixtures/winer/lol-champ-select-v1-session--champ-select-planning.json"), "PLANNING"),
            (include_str!("../../fixtures/winer/lol-champ-select-v1-session--champ-select-ban-pick.json"), "BAN_PICK"),
            (include_str!("../../fixtures/winer/lol-champ-select-v1-session--champ-select-finalization.json"), "FINALIZATION"),
        ] {
            let body: Value = serde_json::from_str(payload).unwrap();
            assert_eq!(super::super::session_recording::sanitize(&body), body);
            let frame = json!([8,"OnJsonApiEvent",{
                "uri":"/lol-champ-select/v1/session","eventType":"Update","data":body
            }]).to_string();
            let parsed = api_event(&frame).unwrap();
            assert_eq!(parsed.data["timer"]["phase"], phase);
            assert!(super::super::champ_select_policy::player(&parsed.data).is_some());
            assert!(parsed.data["actions"].is_array());
            assert!(parsed.data.get("chatDetails").is_none());
        }
    }

    #[test]
    fn whitespace_key_order_and_unicode_do_not_change_event_routing() {
        let event = api_event(
            r#"[8, "OnJsonApiEvent", {
          "data": {"playerResponse": "None", "timer": 1.25, "state": "InProgress", "label":"Игра"},
          "eventType": "Update", "uri": "/lol-matchmaking/v1/ready-check"}]"#,
        )
        .unwrap();
        assert_eq!(event.uri, "/lol-matchmaking/v1/ready-check");
        assert_eq!(ready_timer(&event.data), Some(1.25));
    }

    #[test]
    fn malformed_frames_and_other_topics_cannot_trigger_actions() {
        for message in [
            "null",
            "[8]",
            "[8,",
            r#"[5,"OnJsonApiEvent",{"uri":"/test","eventType":"Update"}]"#,
            r#"[8,"OtherTopic",{"uri":"/test","eventType":"Update"}]"#,
            r#"[8,"OnJsonApiEvent",{"uri":"/test","eventType":"Unknown"}]"#,
        ] {
            assert!(api_event(message).is_none(), "{message}");
        }
        let event = api_event(r#"[8,"OnJsonApiEvent",{"uri":"/other","eventType":"Update","data":{"uri":"/lol-matchmaking/v1/ready-check"}}]"#).unwrap();
        assert_eq!(event.uri, "/other");
    }

    #[test]
    fn completed_declined_and_invalid_timers_do_not_accept() {
        for data in [
            json!({}),
            json!({"state":"Done","playerResponse":"None","timer":1}),
            json!({"state":"InProgress","playerResponse":"Accepted","timer":1}),
            json!({"state":"InProgress","playerResponse":"None","timer":-1}),
            json!({"state":"InProgress","playerResponse":"None","timer":"1"}),
        ] {
            assert_eq!(ready_timer(&data), None);
        }
        assert_eq!(
            ready_timer(&json!({"state":"InProgress","playerResponse":"None","timer":0})),
            Some(0.0)
        );
    }
}
