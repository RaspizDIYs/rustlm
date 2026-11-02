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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
