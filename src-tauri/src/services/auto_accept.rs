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
use crate::services::rune_pages_storage::RunePagesStorage;

pub struct AutoAcceptService {
    riot_client: Arc<RiotClientService>,
    rune_pages_storage: Arc<RunePagesStorage>,
    settings: RwLock<AutomationSettings>,
    /// Controls only auto-accept (match acceptance). Champ-select automation is always active.
    enabled: AtomicBool,
    listeners_running: AtomicBool,
    ws_failures: AtomicU64,
    force_polling: AtomicBool,
    accept_in_progress: AtomicBool,
    has_picked_champion: AtomicBool,
    has_banned_champion: AtomicBool,
    has_set_spells: AtomicBool,
    has_set_runes: AtomicBool,
    last_accepted_timer: Mutex<f64>,
    last_game_id: AtomicI64,
    champ_select_version: AtomicU64,
    cancel_tx: Mutex<Option<tokio::sync::watch::Sender<bool>>>,
    app_handle: Mutex<Option<tauri::AppHandle>>,
}

impl AutoAcceptService {
    pub fn new(riot_client: Arc<RiotClientService>, rune_pages_storage: Arc<RunePagesStorage>) -> Self {
        Self {
            riot_client,
            rune_pages_storage,
            settings: RwLock::new(AutomationSettings::default()),
            enabled: AtomicBool::new(false),
            listeners_running: AtomicBool::new(false),
            ws_failures: AtomicU64::new(0),
            force_polling: AtomicBool::new(false),
            accept_in_progress: AtomicBool::new(false),
            has_picked_champion: AtomicBool::new(false),
            has_banned_champion: AtomicBool::new(false),
            has_set_spells: AtomicBool::new(false),
            has_set_runes: AtomicBool::new(false),
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
        log::info!("[AutoAccept] set_settings: pick1={:?}(id={:?}), ban={:?}(id={:?}), spell1={:?}, spell2={:?}",
            settings.pick_champion1, settings.pick_champion1_id,
            settings.ban_champion, settings.ban_champion_id,
            settings.spell1_id, settings.spell2_id,
        );
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

    /// Toggle auto-accept on/off. This only controls match acceptance.
    /// Background listeners are always running for champ-select automation.
    pub async fn set_enabled_arc(self: &Arc<Self>, enabled: bool) {
        log::info!("[AutoAccept] set_enabled_arc({})", enabled);
        self.enabled.store(enabled, Ordering::SeqCst);

        if enabled {
            let settings = self.settings.read().await;
            log::info!("[AutoAccept] Settings: pick1={:?}(id={:?}), ban={:?}(id={:?}), spell1={:?}, spell2={:?}, runes={}, rune_page='{}'",
                settings.pick_champion1, settings.pick_champion1_id,
                settings.ban_champion, settings.ban_champion_id,
                settings.spell1_id, settings.spell2_id,
                settings.auto_runes_enabled, settings.selected_rune_page_name,
            );
            drop(settings);
        }

        // Ensure listeners are running
        self.ensure_listeners_started().await;
    }

    /// Start background listeners if not already running. Called at app startup and on toggle.
    pub async fn ensure_listeners_started(self: &Arc<Self>) {
        if self.listeners_running.compare_exchange(
            false, true, Ordering::SeqCst, Ordering::SeqCst,
        ).is_err() {
            return; // Already running
        }

        log::info!("[AutoAccept] Starting background listeners");

        let (tx, rx) = tokio::sync::watch::channel(false);
        *self.cancel_tx.lock().await = Some(tx);

        self.ws_failures.store(0, Ordering::SeqCst);
        self.force_polling.store(false, Ordering::SeqCst);
        self.reset_champ_select_state();

        // Start WebSocket listener
        let this = Arc::clone(self);
        let rx_ws = rx.clone();
        tokio::spawn(async move {
            this.run_websocket_listener(rx_ws).await;
        });

        // Start polling listener (fallback)
        let this = Arc::clone(self);
        tokio::spawn(async move {
            this.run_polling_listener(rx).await;
        });
    }

    fn reset_champ_select_state(&self) {
        self.has_picked_champion.store(false, Ordering::SeqCst);
        self.has_banned_champion.store(false, Ordering::SeqCst);
        self.has_set_spells.store(false, Ordering::SeqCst);
        self.has_set_runes.store(false, Ordering::SeqCst);
        self.champ_select_version.fetch_add(1, Ordering::SeqCst);
    }

    async fn run_websocket_listener(&self, mut cancel: tokio::sync::watch::Receiver<bool>) {
        log::info!("[WS] WebSocket listener started");
        loop {
            if *cancel.borrow() {
                log::info!("[WS] WebSocket listener stopped");
                return;
            }

            // Get LCU auth
            let (port, password) = match self.riot_client.get_lcu_auth() {
                Some(auth) => {
                    log::info!("[WS] LCU auth found, port={}", auth.0);
                    auth
                }
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
                    log::error!("[WS] WebSocket error: {}", e);
                    self.riot_client.invalidate_cache();
                    let failures = self.ws_failures.fetch_add(1, Ordering::SeqCst) + 1;
                    if failures >= 3 {
                        self.force_polling.store(true, Ordering::SeqCst);
                        log::warn!("[WS] WebSocket failed {}x, enabling polling fallback", failures);
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

        log::info!("[WS] WebSocket connected to LCU, subscribed to OnJsonApiEvent");

        // Read loop
        loop {
            tokio::select! {
                msg = read.next() => {
                    match msg {
                        Some(Ok(tokio_tungstenite::tungstenite::Message::Text(text))) => {
                            self.handle_ws_message(&text).await;
                        }
                        Some(Ok(tokio_tungstenite::tungstenite::Message::Close(_))) => {
                            log::info!("[WS] WebSocket closed by server");
                            self.riot_client.invalidate_cache();
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
                    log::info!("[WS] WebSocket cancelled");
                    return Ok(());
                }
            }
        }
    }

    async fn handle_ws_message(&self, message: &str) {
        // Filter by URI to avoid false positives from "champ-select" appearing in unrelated events
        let is_ready_check = message.contains("/lol-matchmaking/v1/ready-check");
        let is_champ_select = message.contains("/lol-champ-select/v1/session");

        if is_ready_check || is_champ_select {
            log::info!("[WS] Event: {}...", &message[..message.len().min(300)]);
        }

        // Auto-accept only when enabled toggle is on
        if is_ready_check && self.enabled.load(Ordering::SeqCst) {
            self.handle_ready_check(message).await;
        }

        // Champ-select automation always active (uses individual toggles)
        if is_champ_select {
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
                log::info!("[AutoAccept] Match accepted automatically");
                self.emit_event("match-accepted", "Матч принят автоматически").await;
            }
            Err(e) => {
                log::error!("[AutoAccept] Failed to accept match: {}", e);
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

        self.handle_champ_select_session(session).await;
    }

    async fn handle_champ_select_session(&self, session: &serde_json::Value) {
        let game_id = session.get("gameId").and_then(|g| g.as_i64()).unwrap_or(0);
        let prev_game_id = self.last_game_id.swap(game_id, Ordering::SeqCst);
        if game_id != prev_game_id && prev_game_id != 0 {
            log::info!("[ChampSelect] New game session detected (gameId={}), resetting state", game_id);
            self.reset_champ_select_state();
        }

        let version = self.champ_select_version.load(Ordering::SeqCst);

        let my_cell = session.get("localPlayerCellId").and_then(|c| c.as_i64()).unwrap_or(-1);
        if my_cell < 0 {
            return;
        }

        let settings = self.settings.read().await.clone();

        // Set rune page once at the start of champ select
        if settings.auto_runes_enabled {
            if self.has_set_runes.compare_exchange(
                false, true, Ordering::SeqCst, Ordering::SeqCst,
            ).is_ok() {
                log::info!("[ChampSelect] Setting rune page '{}'", settings.selected_rune_page_name);
                self.set_rune_page(&settings).await;
            }
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

        // Log all my actions for diagnostics
        let mut my_actions_summary = Vec::new();
        if let Some(actions) = session.get("actions").and_then(|a| a.as_array()) {
            for action_group in actions {
                if let Some(group) = action_group.as_array() {
                    for action in group {
                        let actor = action.get("actorCellId").and_then(|a| a.as_i64()).unwrap_or(-1);
                        if actor == my_cell {
                            let action_type = action.get("type").and_then(|t| t.as_str()).unwrap_or("?");
                            let completed = action.get("completed").and_then(|c| c.as_bool()).unwrap_or(false);
                            let is_in_progress = action.get("isInProgress").and_then(|i| i.as_bool()).unwrap_or(false);
                            let action_id = action.get("id").and_then(|i| i.as_i64()).unwrap_or(0);
                            let champ_id = action.get("championId").and_then(|c| c.as_i64()).unwrap_or(0);
                            my_actions_summary.push(format!("{}(id={},champ={},prog={},done={})", action_type, action_id, champ_id, is_in_progress, completed));
                        }
                    }
                }
            }
        }
        if !my_actions_summary.is_empty() {
            log::info!("[ChampSelect] myCell={}, actions: [{}], has_picked={}, has_banned={}, has_spells={}",
                my_cell, my_actions_summary.join(", "),
                self.has_picked_champion.load(Ordering::SeqCst),
                self.has_banned_champion.load(Ordering::SeqCst),
                self.has_set_spells.load(Ordering::SeqCst),
            );
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

                        if action_type == "ban" && is_in_progress && settings.auto_ban_enabled {
                            if self.has_banned_champion.compare_exchange(
                                false, true, Ordering::SeqCst, Ordering::SeqCst,
                            ).is_ok() {
                                log::info!("[ChampSelect] Ban phase, my turn (actionId={})", action_id);
                                if let Some(ref ban) = settings.ban_champion {
                                    if !ban.is_empty() {
                                        let ban_id = settings.ban_champion_id.unwrap_or(0);
                                        if ban_id > 0 {
                                            log::info!("[ChampSelect] Banning '{}' (id={})", ban, ban_id);
                                            let success = self.do_champion_action(action_id, ban_id, true).await;
                                            if !success {
                                                // Reset flag so next WS event can retry
                                                self.has_banned_champion.store(false, Ordering::SeqCst);
                                                log::warn!("[ChampSelect] Ban failed, will retry on next event");
                                            }
                                        } else {
                                            log::warn!("[ChampSelect] Ban champion '{}' has no resolved ID!", ban);
                                            self.has_banned_champion.store(false, Ordering::SeqCst);
                                        }
                                    }
                                } else {
                                    log::info!("[ChampSelect] No ban champion configured");
                                }
                            }
                        }

                        if action_type == "pick" && is_in_progress && settings.auto_pick_enabled {
                            if self.has_picked_champion.compare_exchange(
                                false, true, Ordering::SeqCst, Ordering::SeqCst,
                            ).is_ok() {
                                log::info!("[ChampSelect] Pick phase, my turn (actionId={})", action_id);

                                // Get pickable champion IDs to check availability
                                let pickable_ids = match self.riot_client.lcu_get("/lol-champ-select/v1/pickable-champion-ids").await {
                                    Ok(body) => {
                                        match serde_json::from_str::<Vec<i64>>(&body) {
                                            Ok(ids) => {
                                                log::info!("[ChampSelect] {} pickable champions available", ids.len());
                                                ids
                                            }
                                            Err(_) => {
                                                log::warn!("[ChampSelect] Failed to parse pickable champions: {}", &body[..body.len().min(200)]);
                                                Vec::new()
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        log::warn!("[ChampSelect] Failed to get pickable champions: {}", e);
                                        Vec::new()
                                    }
                                };

                                let picks = [
                                    (&settings.pick_champion1, settings.pick_champion1_id),
                                    (&settings.pick_champion2, settings.pick_champion2_id),
                                    (&settings.pick_champion3, settings.pick_champion3_id),
                                ];

                                let mut picked = false;
                                for (name, id_opt) in &picks {
                                    if let Some(name) = name {
                                        if !name.is_empty() {
                                            let pick_id = id_opt.unwrap_or(0) as i64;
                                            if pick_id <= 0 {
                                                log::warn!("[ChampSelect] Pick champion '{}' has no resolved ID!", name);
                                                continue;
                                            }
                                            if banned_ids.contains(&pick_id) {
                                                log::info!("[ChampSelect] Pick '{}' (id={}) is banned, trying next", name, pick_id);
                                                continue;
                                            }
                                            if !pickable_ids.is_empty() && !pickable_ids.contains(&pick_id) {
                                                log::warn!("[ChampSelect] Pick '{}' (id={}) is NOT in pickable list (not owned/free rotation?), trying next", name, pick_id);
                                                continue;
                                            }
                                            log::info!("[ChampSelect] Picking '{}' (id={})", name, pick_id);
                                            let success = self.do_champion_action(action_id, pick_id as i32, false).await;
                                            if success {
                                                picked = true;
                                                break;
                                            }
                                            // If this pick failed, try next champion
                                            log::warn!("[ChampSelect] Pick '{}' failed, trying next", name);
                                        }
                                    }
                                }
                                if !picked {
                                    // Reset flag so next WS event can retry
                                    self.has_picked_champion.store(false, Ordering::SeqCst);
                                    log::warn!("[ChampSelect] No pick succeeded, will retry on next event");
                                }
                            }
                        }

                        // Set summoner spells (independent of pick)
                        if action_type == "pick" && is_in_progress && settings.auto_spells_enabled {
                            if self.has_set_spells.compare_exchange(
                                false, true, Ordering::SeqCst, Ordering::SeqCst,
                            ).is_ok() {
                                log::info!("[ChampSelect] Setting spells: D={:?}, F={:?}", settings.spell1_id, settings.spell2_id);
                                self.set_summoner_spells(&settings).await;
                            }
                        }
                    }
                }
            }
        }
    }

    /// Perform a champion action (ban or pick) — single attempt.
    /// Returns true if the action was verified as completed.
    /// On failure, the caller should reset the flag so the next WS event can retry.
    async fn do_champion_action(&self, action_id: i64, champion_id: i32, is_ban: bool) -> bool {
        let action_str = if is_ban { "ban" } else { "pick" };
        let endpoint = format!("/lol-champ-select/v1/session/actions/{}", action_id);

        // Step 1: PATCH to hover (set championId without completing)
        let hover_body = format!("{{\"championId\":{}}}", champion_id);
        log::info!("[ChampSelect] {} step1 hover: PATCH {} body={}", action_str, endpoint, hover_body);

        match self.riot_client.lcu_patch(&endpoint, &hover_body).await {
            Ok(resp) => log::info!("[ChampSelect] {} hover ok: '{}'", action_str, resp),
            Err(e) => {
                log::error!("[ChampSelect] {} hover FAILED: {}", action_str, e);
                return false;
            }
        }

        // Wait for hover to register
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Verify hover took effect
        let hover_verified = match self.riot_client.lcu_get("/lol-champ-select/v1/session").await {
            Ok(session_str) => {
                if let Ok(session) = serde_json::from_str::<serde_json::Value>(&session_str) {
                    Self::check_action_champion(&session, action_id, champion_id as i64)
                } else {
                    false
                }
            }
            Err(e) => {
                log::warn!("[ChampSelect] {} verify GET failed: {}", action_str, e);
                true // optimistic — continue anyway
            }
        };

        if !hover_verified {
            log::warn!("[ChampSelect] {} hover NOT verified, will retry on next event", action_str);
            return false;
        }

        log::info!("[ChampSelect] {} hover verified, proceeding to lock-in", action_str);

        // Step 2: PATCH with completed=true to lock in
        let complete_body = format!("{{\"championId\":{},\"completed\":true}}", champion_id);
        log::info!("[ChampSelect] {} step2 lock-in: PATCH {} body={}", action_str, endpoint, complete_body);

        match self.riot_client.lcu_patch(&endpoint, &complete_body).await {
            Ok(resp) => log::info!("[ChampSelect] {} lock-in ok: '{}'", action_str, resp),
            Err(e) => {
                log::error!("[ChampSelect] {} lock-in FAILED: {}", action_str, e);
                return false;
            }
        }

        // Verify completion
        tokio::time::sleep(Duration::from_millis(300)).await;

        match self.riot_client.lcu_get("/lol-champ-select/v1/session").await {
            Ok(session_str) => {
                if let Ok(session) = serde_json::from_str::<serde_json::Value>(&session_str) {
                    if Self::check_action_completed(&session, action_id) {
                        log::info!("[ChampSelect] {} VERIFIED completed!", action_str);
                        return true;
                    }
                    log::warn!("[ChampSelect] {} NOT completed after lock-in, will retry on next event", action_str);
                }
            }
            Err(e) => log::warn!("[ChampSelect] {} completion verify failed: {}", action_str, e),
        }

        false
    }

    /// Check if an action has the expected championId set
    fn check_action_champion(session: &serde_json::Value, action_id: i64, expected_champ: i64) -> bool {
        if let Some(actions) = session.get("actions").and_then(|a| a.as_array()) {
            for group in actions {
                if let Some(arr) = group.as_array() {
                    for action in arr {
                        let id = action.get("id").and_then(|i| i.as_i64()).unwrap_or(-1);
                        if id == action_id {
                            let champ = action.get("championId").and_then(|c| c.as_i64()).unwrap_or(0);
                            log::info!("[ChampSelect] verify action {}: championId={} (expected={})", action_id, champ, expected_champ);
                            return champ == expected_champ;
                        }
                    }
                }
            }
        }
        false
    }

    /// Check if an action is completed
    fn check_action_completed(session: &serde_json::Value, action_id: i64) -> bool {
        if let Some(actions) = session.get("actions").and_then(|a| a.as_array()) {
            for group in actions {
                if let Some(arr) = group.as_array() {
                    for action in arr {
                        let id = action.get("id").and_then(|i| i.as_i64()).unwrap_or(-1);
                        if id == action_id {
                            let completed = action.get("completed").and_then(|c| c.as_bool()).unwrap_or(false);
                            let champ = action.get("championId").and_then(|c| c.as_i64()).unwrap_or(0);
                            log::info!("[ChampSelect] verify action {}: championId={}, completed={}", action_id, champ, completed);
                            return completed;
                        }
                    }
                }
            }
        }
        false
    }

    async fn set_rune_page(&self, settings: &AutomationSettings) {
        if settings.selected_rune_page_name.is_empty() {
            return;
        }

        // Find the rune page by name from local storage
        let pages = self.rune_pages_storage.load_all();
        let page = match pages.iter().find(|p| p.name == settings.selected_rune_page_name) {
            Some(p) => p,
            None => {
                log::warn!("[ChampSelect] Rune page '{}' not found", settings.selected_rune_page_name);
                return;
            }
        };

        // Build perk IDs array: keystone + 3 primary + 2 secondary + 3 stat mods
        let selected_perk_ids = serde_json::json!([
            page.primary_keystone_id,
            page.primary_slot1_id,
            page.primary_slot2_id,
            page.primary_slot3_id,
            page.secondary_slot1_id,
            page.secondary_slot2_id,
            page.stat_mod1_id,
            page.stat_mod2_id,
            page.stat_mod3_id,
        ]);

        // First, get current rune pages from LCU to find one we can overwrite
        let lcu_pages = match self.riot_client.lcu_get("/lol-perks/v1/pages").await {
            Ok(body) => serde_json::from_str::<serde_json::Value>(&body).unwrap_or_default(),
            Err(e) => {
                log::error!("[ChampSelect] Failed to get LCU rune pages: {}", e);
                return;
            }
        };

        // Look for an existing RustLM page to overwrite, or find a deletable page
        let mut rustlm_page_id: Option<i64> = None;
        let mut deletable_page_id: Option<i64> = None;

        if let Some(arr) = lcu_pages.as_array() {
            for p in arr {
                let name = p.get("name").and_then(|n| n.as_str()).unwrap_or("");
                let id = p.get("id").and_then(|i| i.as_i64()).unwrap_or(0);
                let is_deletable = p.get("isDeletable").and_then(|d| d.as_bool()).unwrap_or(false);
                if name == "RustLM" {
                    rustlm_page_id = Some(id);
                    break;
                }
                if is_deletable && deletable_page_id.is_none() {
                    deletable_page_id = Some(id);
                }
            }
        }

        let body = serde_json::json!({
            "name": "RustLM",
            "primaryStyleId": page.primary_path_id,
            "subStyleId": page.secondary_path_id,
            "selectedPerkIds": selected_perk_ids,
            "current": true,
        });

        if let Some(page_id) = rustlm_page_id {
            // Overwrite existing RustLM page
            let endpoint = format!("/lol-perks/v1/pages/{}", page_id);
            match self.riot_client.lcu_put(&endpoint, &body.to_string()).await {
                Ok(_) => log::info!("[ChampSelect] Rune page '{}' updated in LCU", settings.selected_rune_page_name),
                Err(e) => log::error!("[ChampSelect] Failed to update rune page: {}", e),
            }
        } else {
            // Delete a page to make room if needed, then create new
            if let Some(del_id) = deletable_page_id {
                let _ = self.riot_client.lcu_delete(&format!("/lol-perks/v1/pages/{}", del_id)).await;
            }
            match self.riot_client.lcu_post("/lol-perks/v1/pages", &body.to_string()).await {
                Ok(_) => log::info!("[ChampSelect] Rune page '{}' created in LCU", settings.selected_rune_page_name),
                Err(e) => log::error!("[ChampSelect] Failed to create rune page: {}", e),
            }
        }
    }

    async fn set_summoner_spells(&self, settings: &AutomationSettings) {
        let spell1 = settings.spell1_id.filter(|&id| id > 0);
        let spell2 = settings.spell2_id.filter(|&id| id > 0);

        if spell1.is_none() && spell2.is_none() {
            log::info!("[ChampSelect] No spells configured, skipping");
            return;
        }

        let mut body = serde_json::Map::new();
        if let Some(id) = spell1 {
            body.insert("spell1Id".to_string(), serde_json::json!(id));
        }
        if let Some(id) = spell2 {
            body.insert("spell2Id".to_string(), serde_json::json!(id));
        }

        match self.riot_client.lcu_patch(
            "/lol-champ-select/v1/session/my-selection",
            &serde_json::Value::Object(body).to_string(),
        ).await {
            Ok(_) => log::info!("[ChampSelect] Spells set: D={:?}, F={:?}", spell1, spell2),
            Err(e) => log::error!("[ChampSelect] Failed to set spells: {}", e),
        }
    }

    async fn run_polling_listener(&self, mut cancel: tokio::sync::watch::Receiver<bool>) {
        log::info!("[Polling] Polling listener started (fallback)");
        let mut poll_count: u64 = 0;
        let mut last_lcu_ok = false;

        loop {
            if *cancel.borrow() {
                log::info!("[Polling] Polling listener stopped");
                return;
            }

            // Only poll when WS is failing (force_polling) or as periodic fallback
            if !self.force_polling.load(Ordering::SeqCst) {
                // Even when WS is primary, do a slow poll every 5s as safety net
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(5)) => {},
                    _ = cancel.changed() => return,
                }
            }

            poll_count += 1;

            // Check LCU connectivity
            let lcu_auth = self.riot_client.get_lcu_auth();
            if lcu_auth.is_none() {
                if last_lcu_ok || poll_count == 1 {
                    log::warn!("[Polling] LCU not connected (no lockfile/auth). Waiting...");
                    last_lcu_ok = false;
                }
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(3)) => continue,
                    _ = cancel.changed() => return,
                }
            }

            if !last_lcu_ok {
                log::info!("[Polling] LCU connected, polling active");
                last_lcu_ok = true;
            }

            // Poll ready-check (only when auto-accept is enabled)
            if self.enabled.load(Ordering::SeqCst) {
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
                    Err(_) => {} // Expected when not in queue
                }
            }

            // Poll champ-select session (always active for auto-pick/ban/spells/runes)
            match self.riot_client.lcu_get("/lol-champ-select/v1/session").await {
                Ok(body) => {
                    if let Ok(session) = serde_json::from_str::<serde_json::Value>(&body) {
                        let game_id = session.get("gameId").and_then(|g| g.as_i64()).unwrap_or(0);
                        if game_id > 0 {
                            self.handle_champ_select_session(&session).await;
                        }
                    }
                }
                Err(_) => {} // Expected when not in champ select (404)
            }

            // Faster polling when WS is down
            let delay = if self.force_polling.load(Ordering::SeqCst) { 300 } else { 5000 };
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_millis(delay)) => {},
                _ = cancel.changed() => return,
            }
        }
    }
}
