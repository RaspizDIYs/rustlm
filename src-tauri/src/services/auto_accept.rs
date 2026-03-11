use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::{Mutex, RwLock};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::header;

use tauri::Emitter;

use crate::error::AppError;
use crate::models::automation::AutomationSettings;
use crate::services::riot_client::RiotClientService;

pub struct AutoAcceptService {
    riot_client: Arc<RiotClientService>,
    settings: RwLock<AutomationSettings>,
    enabled: AtomicBool,
    ws_failures: AtomicU64,
    force_polling: AtomicBool,
    accept_in_progress: AtomicBool,
    has_picked_champion: AtomicBool,
    has_banned_champion: AtomicBool,
    has_set_spells: AtomicBool,
    last_accepted_timer: Mutex<f64>,
    last_game_id: AtomicI64,
    champ_select_version: AtomicU64,
    cancel_tx: Mutex<Option<tokio::sync::watch::Sender<bool>>>,
    app_handle: Mutex<Option<tauri::AppHandle>>,
}

impl AutoAcceptService {
    pub fn new(riot_client: Arc<RiotClientService>) -> Self {
        Self {
            riot_client,
            settings: RwLock::new(AutomationSettings::default()),
            enabled: AtomicBool::new(false),
            ws_failures: AtomicU64::new(0),
            force_polling: AtomicBool::new(false),
            accept_in_progress: AtomicBool::new(false),
            has_picked_champion: AtomicBool::new(false),
            has_banned_champion: AtomicBool::new(false),
            has_set_spells: AtomicBool::new(false),
            last_accepted_timer: Mutex::new(-1.0),
            last_game_id: AtomicI64::new(0),
            champ_select_version: AtomicU64::new(0),
            cancel_tx: Mutex::new(None),
            app_handle: Mutex::new(None),
        }
    }

    pub async fn set_app_handle(&self, handle: tauri::AppHandle) {
        *self.app_handle.lock().await = Some(handle);
    }

    pub async fn set_settings(&self, settings: AutomationSettings) {
        *self.settings.write().await = settings;
    }

    pub async fn get_settings(&self) -> AutomationSettings {
        self.settings.read().await.clone()
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::SeqCst)
    }

    async fn emit_event(&self, event: &str, payload: &str) {
        if let Some(handle) = self.app_handle.lock().await.as_ref() {
            let _ = handle.emit(event, payload.to_string());
        }
    }

    /// Call on Arc<AutoAcceptService> to start/stop background tasks.
    pub async fn set_enabled_arc(self: &Arc<Self>, enabled: bool) {
        self.enabled.store(enabled, Ordering::SeqCst);
        if enabled {
            self.start().await;
        } else {
            self.stop().await;
        }
    }

    async fn start(self: &Arc<Self>) {
        self.stop().await;
        let (tx, rx) = tokio::sync::watch::channel(false);
        *self.cancel_tx.lock().await = Some(tx);

        // Reset state
        self.ws_failures.store(0, Ordering::SeqCst);
        self.force_polling.store(false, Ordering::SeqCst);
        self.reset_champ_select_state();

        // Start WebSocket listener
        let this = Arc::clone(self);
        let rx_ws = rx.clone();
        tokio::spawn(async move {
            this.run_websocket_listener(rx_ws).await;
        });

        // Start polling listener
        let this = Arc::clone(self);
        tokio::spawn(async move {
            this.run_polling_listener(rx).await;
        });
    }

    async fn stop(&self) {
        if let Some(tx) = self.cancel_tx.lock().await.take() {
            let _ = tx.send(true);
        }
    }

    pub async fn shutdown(&self) {
        self.enabled.store(false, Ordering::SeqCst);
        self.stop().await;
    }

    fn reset_champ_select_state(&self) {
        self.has_picked_champion.store(false, Ordering::SeqCst);
        self.has_banned_champion.store(false, Ordering::SeqCst);
        self.has_set_spells.store(false, Ordering::SeqCst);
        self.champ_select_version.fetch_add(1, Ordering::SeqCst);
    }

    async fn run_websocket_listener(&self, mut cancel: tokio::sync::watch::Receiver<bool>) {
        loop {
            if *cancel.borrow() || !self.enabled.load(Ordering::SeqCst) {
                return;
            }

            let settings = self.settings.read().await;
            let method = settings.auto_accept_method.clone();
            drop(settings);

            if method == "Polling" {
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(1)) => continue,
                    _ = cancel.changed() => return,
                }
            }

            // Get LCU auth (synchronous)
            let (port, password) = match self.riot_client.get_lcu_auth() {
                Some(auth) => auth,
                None => {
                    tokio::select! {
                        _ = tokio::time::sleep(Duration::from_secs(3)) => continue,
                        _ = cancel.changed() => return,
                    }
                }
            };

            // Build WebSocket URL
            let ws_url = format!("wss://127.0.0.1:{}/", port);
            let auth = base64::engine::general_purpose::STANDARD.encode(
                format!("riot:{}", password),
            );

            // Connect
            match self.ws_connect(&ws_url, &auth, &mut cancel).await {
                Ok(()) => {
                    self.ws_failures.store(0, Ordering::SeqCst);
                    self.force_polling.store(false, Ordering::SeqCst);
                }
                Err(e) => {
                    log::error!("WebSocket error: {}", e);
                    let failures = self.ws_failures.fetch_add(1, Ordering::SeqCst) + 1;
                    if failures >= 3 {
                        self.force_polling.store(true, Ordering::SeqCst);
                        log::warn!("WebSocket failed 3+ times, enabling polling fallback");
                    }
                }
            }

            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(5)) => {},
                _ = cancel.changed() => return,
            }
        }
    }

    async fn ws_connect(
        &self,
        url: &str,
        auth: &str,
        cancel: &mut tokio::sync::watch::Receiver<bool>,
    ) -> Result<(), AppError> {
        let mut request = url.into_client_request()
            .map_err(|e| AppError::Custom(e.to_string()))?;

        request.headers_mut().insert(
            header::AUTHORIZATION,
            format!("Basic {}", auth).parse().unwrap(),
        );

        let connector = tokio_tungstenite::Connector::NativeTls(
            native_tls::TlsConnector::builder()
                .danger_accept_invalid_certs(true)
                .build()
                .map_err(|e| AppError::Custom(e.to_string()))?,
        );

        let (ws_stream, _) = tokio_tungstenite::connect_async_tls_with_config(
            request,
            None,
            false,
            Some(connector),
        )
        .await
        .map_err(|e| AppError::Custom(format!("WS connect failed: {}", e)))?;

        let (mut write, mut read) = ws_stream.split();

        // Subscribe to events
        let sub_msg = tokio_tungstenite::tungstenite::Message::Text(
            "[5, \"OnJsonApiEvent\"]".into(),
        );
        write.send(sub_msg).await
            .map_err(|e| AppError::Custom(e.to_string()))?;

        log::info!("WebSocket connected, subscribed to LCU events");

        // Read loop
        loop {
            tokio::select! {
                msg = read.next() => {
                    match msg {
                        Some(Ok(tokio_tungstenite::tungstenite::Message::Text(text))) => {
                            self.handle_ws_message(&text).await;
                        }
                        Some(Ok(tokio_tungstenite::tungstenite::Message::Close(_))) => {
                            log::info!("WebSocket closed by server");
                            return Ok(());
                        }
                        Some(Err(e)) => {
                            return Err(AppError::Custom(format!("WS read error: {}", e)));
                        }
                        None => {
                            return Ok(());
                        }
                        _ => {}
                    }
                }
                _ = cancel.changed() => {
                    log::info!("WebSocket cancelled");
                    return Ok(());
                }
            }
        }
    }

    async fn handle_ws_message(&self, message: &str) {
        if !self.enabled.load(Ordering::SeqCst) {
            return;
        }

        if message.contains("ready-check") {
            self.handle_ready_check(message).await;
        }

        if message.contains("champ-select") {
            self.handle_champ_select(message).await;
        }
    }

    async fn handle_ready_check(&self, message: &str) {
        if !message.contains("\"state\":\"InProgress\"") {
            return;
        }
        if !message.contains("\"playerResponse\":\"None\"") {
            return;
        }

        let timer = Self::extract_timer(message);
        {
            let mut last = self.last_accepted_timer.lock().await;
            if (*last - timer).abs() < 0.5 && *last > 0.0 {
                return;
            }
            *last = timer;
        }

        self.accept_match().await;
    }

    fn extract_timer(message: &str) -> f64 {
        let re = regex::Regex::new(r#""timer":([\d.]+)"#).unwrap();
        re.captures(message)
            .and_then(|caps| caps.get(1))
            .and_then(|m| m.as_str().parse().ok())
            .unwrap_or(-1.0)
    }

    async fn accept_match(&self) {
        if self.accept_in_progress.compare_exchange(
            false, true, Ordering::SeqCst, Ordering::SeqCst,
        ).is_err() {
            return;
        }

        let result = self.riot_client.lcu_post(
            "/lol-matchmaking/v1/ready-check/accept",
            "{}",
        ).await;

        self.accept_in_progress.store(false, Ordering::SeqCst);

        match result {
            Ok(_) => {
                log::info!("Match accepted automatically");
                self.emit_event("match-accepted", "Матч принят автоматически").await;
            }
            Err(e) => {
                log::error!("Failed to accept match: {}", e);
            }
        }
    }

    async fn handle_champ_select(&self, message: &str) {
        // Parse WAMP message: [8, "OnJsonApiEvent", {...}]
        let parsed: Result<serde_json::Value, _> = serde_json::from_str(message);
        let data = match parsed {
            Ok(val) => {
                if let Some(arr) = val.as_array() {
                    if arr.len() >= 3 {
                        arr[2].clone()
                    } else {
                        return;
                    }
                } else {
                    return;
                }
            }
            Err(_) => return,
        };

        let uri = data.get("uri").and_then(|u| u.as_str()).unwrap_or("");
        if !uri.contains("/lol-champ-select/v1/session") {
            return;
        }

        let event_type = data.get("eventType").and_then(|e| e.as_str()).unwrap_or("");
        if event_type == "Delete" {
            self.reset_champ_select_state();
            return;
        }

        let session = match data.get("data") {
            Some(d) => d,
            None => return,
        };

        let game_id = session.get("gameId").and_then(|g| g.as_i64()).unwrap_or(0);
        let prev_game_id = self.last_game_id.swap(game_id, Ordering::SeqCst);
        if game_id != prev_game_id && prev_game_id != 0 {
            self.reset_champ_select_state();
        }

        let version = self.champ_select_version.load(Ordering::SeqCst);

        let my_cell = session.get("localPlayerCellId").and_then(|c| c.as_i64()).unwrap_or(-1);
        if my_cell < 0 {
            return;
        }

        let settings = self.settings.read().await.clone();
        if !settings.is_enabled {
            return;
        }

        // Collect banned champion IDs
        let mut banned_ids: Vec<i64> = Vec::new();
        if let Some(actions) = session.get("actions").and_then(|a| a.as_array()) {
            for action_group in actions {
                if let Some(group) = action_group.as_array() {
                    for action in group {
                        let action_type = action.get("type").and_then(|t| t.as_str()).unwrap_or("");
                        let completed = action.get("completed").and_then(|c| c.as_bool()).unwrap_or(false);
                        let champ_id = action.get("championId").and_then(|c| c.as_i64()).unwrap_or(0);
                        if action_type == "ban" && completed && champ_id > 0 {
                            banned_ids.push(champ_id);
                        }
                    }
                }
            }
        }

        // Process my actions
        if let Some(actions) = session.get("actions").and_then(|a| a.as_array()) {
            for action_group in actions {
                if let Some(group) = action_group.as_array() {
                    for action in group {
                        let actor = action.get("actorCellId").and_then(|a| a.as_i64()).unwrap_or(-1);
                        if actor != my_cell {
                            continue;
                        }

                        let action_type = action.get("type").and_then(|t| t.as_str()).unwrap_or("");
                        let completed = action.get("completed").and_then(|c| c.as_bool()).unwrap_or(false);
                        let action_id = action.get("id").and_then(|i| i.as_i64()).unwrap_or(0);
                        let is_in_progress = action.get("isInProgress").and_then(|i| i.as_bool()).unwrap_or(false);

                        if completed {
                            continue;
                        }

                        if version != self.champ_select_version.load(Ordering::SeqCst) {
                            return;
                        }

                        if action_type == "ban" && is_in_progress {
                            if self.has_banned_champion.compare_exchange(
                                false, true, Ordering::SeqCst, Ordering::SeqCst,
                            ).is_ok() {
                                if let Some(ref ban) = settings.ban_champion {
                                    if !ban.is_empty() {
                                        let ban_id = settings.ban_champion_id.unwrap_or(0);
                                        if ban_id > 0 {
                                            self.do_champion_action(action_id, ban_id, true).await;
                                        }
                                    }
                                }
                            }
                        }

                        if action_type == "pick" && is_in_progress {
                            if self.has_picked_champion.compare_exchange(
                                false, true, Ordering::SeqCst, Ordering::SeqCst,
                            ).is_ok() {
                                let picks = [
                                    (&settings.pick_champion1, settings.pick_champion1_id),
                                    (&settings.pick_champion2, settings.pick_champion2_id),
                                    (&settings.pick_champion3, settings.pick_champion3_id),
                                ];

                                for (name, id_opt) in &picks {
                                    if let Some(name) = name {
                                        if !name.is_empty() {
                                            let pick_id = id_opt.unwrap_or(0) as i64;
                                            if pick_id > 0 && !banned_ids.contains(&pick_id) {
                                                self.do_champion_action(action_id, pick_id as i32, false).await;
                                                break;
                                            }
                                        }
                                    }
                                }

                                // Set summoner spells after pick
                                if self.has_set_spells.compare_exchange(
                                    false, true, Ordering::SeqCst, Ordering::SeqCst,
                                ).is_ok() {
                                    self.set_summoner_spells(&settings).await;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    async fn do_champion_action(&self, action_id: i64, champion_id: i32, is_ban: bool) {
        let body = serde_json::json!({
            "championId": champion_id,
            "completed": true
        });

        let endpoint = format!("/lol-champ-select/v1/session/actions/{}", action_id);
        let result = self.riot_client.lcu_patch(&endpoint, &body.to_string()).await;

        match result {
            Ok(_) => {
                let action_str = if is_ban { "banned" } else { "picked" };
                log::info!("Champion {} (ID: {})", action_str, champion_id);
            }
            Err(e) => {
                log::error!("Failed to {} champion: {}", if is_ban { "ban" } else { "pick" }, e);
            }
        }
    }

    async fn set_summoner_spells(&self, settings: &AutomationSettings) {
        if let Some(spell1_id) = settings.spell1_id {
            if spell1_id > 0 {
                let body = serde_json::json!({"spell1Id": spell1_id});
                let _ = self.riot_client.lcu_patch(
                    "/lol-champ-select/v1/session/my-selection",
                    &body.to_string(),
                ).await;
            }
        }
        if let Some(spell2_id) = settings.spell2_id {
            if spell2_id > 0 {
                let body = serde_json::json!({"spell2Id": spell2_id});
                let _ = self.riot_client.lcu_patch(
                    "/lol-champ-select/v1/session/my-selection",
                    &body.to_string(),
                ).await;
            }
        }
    }

    async fn run_polling_listener(&self, mut cancel: tokio::sync::watch::Receiver<bool>) {
        loop {
            if *cancel.borrow() || !self.enabled.load(Ordering::SeqCst) {
                return;
            }

            let settings = self.settings.read().await;
            let method = settings.auto_accept_method.clone();
            drop(settings);

            let should_poll = method == "Polling" || self.force_polling.load(Ordering::SeqCst);
            if !should_poll {
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(1)) => continue,
                    _ = cancel.changed() => return,
                }
            }

            match self.riot_client.lcu_get("/lol-matchmaking/v1/ready-check").await {
                Ok(body) => {
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body) {
                        let state = json.get("state").and_then(|s| s.as_str()).unwrap_or("");
                        let response = json.get("playerResponse").and_then(|r| r.as_str()).unwrap_or("");
                        if state == "InProgress" && response == "None" {
                            let timer = json.get("timer").and_then(|t| t.as_f64()).unwrap_or(-1.0);
                            let mut last = self.last_accepted_timer.lock().await;
                            if (*last - timer).abs() >= 0.5 || *last < 0.0 {
                                *last = timer;
                                drop(last);
                                self.accept_match().await;
                            }
                        }
                    }
                }
                Err(_) => {}
            }

            tokio::select! {
                _ = tokio::time::sleep(Duration::from_millis(300)) => {},
                _ = cancel.changed() => return,
            }
        }
    }
}
