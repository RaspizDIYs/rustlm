use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use tokio::sync::{Mutex, RwLock};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::header;

use tauri::Emitter;

use crate::error::AppError;
use crate::models::automation::AutomationSettings;
use crate::services::champ_select_policy as policy;
use crate::services::draft_coordination as coordination;
use crate::services::lcu_protocol;
use crate::services::riot_client::RiotClientService;
use crate::services::rune_pages_storage::RunePagesStorage;

struct AcceptAttempt<'a>(&'a AtomicBool);

impl Drop for AcceptAttempt<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

pub struct AutoAcceptService {
    riot_client: Arc<RiotClientService>,
    rune_pages_storage: Arc<RunePagesStorage>,
    settings: RwLock<AutomationSettings>,
    settings_revision: AtomicU64,
    selection_lock: Mutex<()>,
    bench_retry_at: Mutex<std::time::Instant>,
    bench_progress: Mutex<policy::BenchProgress>,
    loadout_retry_at: Mutex<std::time::Instant>,
    /// Controls only auto-accept (match acceptance). Champ-select automation is always active.
    enabled: AtomicBool,
    listeners_running: AtomicBool,
    ws_failures: AtomicU64,
    force_polling: AtomicBool,
    accept_in_progress: AtomicBool,
    has_picked_champion: AtomicBool,
    has_banned_champion: AtomicBool,
    ban_fallback: Mutex<policy::BanFallback>,
    has_set_spells: AtomicBool,
    has_set_runes: AtomicBool,
    last_game_id: AtomicI64,
    last_loadout_profile: Mutex<String>,
    champ_select_active: AtomicBool,
    champ_select_version: AtomicU64,
    cancel_tx: Mutex<Option<tokio::sync::watch::Sender<bool>>>,
    app_handle: Mutex<Option<tauri::AppHandle>>,
}

impl AutoAcceptService {
    pub fn new(
        riot_client: Arc<RiotClientService>,
        rune_pages_storage: Arc<RunePagesStorage>,
    ) -> Self {
        Self {
            riot_client,
            rune_pages_storage,
            settings: RwLock::new(AutomationSettings::default()),
            settings_revision: AtomicU64::new(0),
            selection_lock: Mutex::new(()),
            bench_retry_at: Mutex::new(std::time::Instant::now()),
            bench_progress: Mutex::new(policy::BenchProgress::default()),
            loadout_retry_at: Mutex::new(std::time::Instant::now()),
            enabled: AtomicBool::new(false),
            listeners_running: AtomicBool::new(false),
            ws_failures: AtomicU64::new(0),
            force_polling: AtomicBool::new(false),
            accept_in_progress: AtomicBool::new(false),
            has_picked_champion: AtomicBool::new(false),
            has_banned_champion: AtomicBool::new(false),
            ban_fallback: Mutex::new(policy::BanFallback::default()),
            has_set_spells: AtomicBool::new(false),
            has_set_runes: AtomicBool::new(false),
            last_game_id: AtomicI64::new(0),
            last_loadout_profile: Mutex::new(String::new()),
            champ_select_active: AtomicBool::new(false),
            champ_select_version: AtomicU64::new(0),
            cancel_tx: Mutex::new(None),
            app_handle: Mutex::new(None),
        }
    }

    pub async fn set_app_handle(&self, handle: tauri::AppHandle) {
        *self.app_handle.lock().await = Some(handle);
    }

    pub async fn set_settings(&self, settings: AutomationSettings) -> u64 {
        coordination::set_enabled(false);
        log::info!("[AutoAccept] set_settings: pick1={:?}(id={:?}), ban={:?}(id={:?}), spell1={:?}, spell2={:?}",
            settings.pick_champion1, settings.pick_champion1_id,
            settings.ban_champion, settings.ban_champion_id,
            settings.spell1_id, settings.spell2_id,
        );
        let mut current = self.settings.write().await;
        *current = settings;
        self.settings_revision.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub async fn refresh_settings_if_unchanged(&self, settings: AutomationSettings, revision: u64) {
        let mut current = self.settings.write().await;
        if self.settings_revision.load(Ordering::SeqCst) == revision {
            coordination::set_enabled(false);
            *current = settings;
            self.settings_revision.fetch_add(1, Ordering::SeqCst);
        }
    }

    pub async fn get_settings(&self) -> AutomationSettings {
        let mut settings = self.settings.read().await.clone();
        settings.is_enabled = self.is_enabled();
        settings
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::SeqCst)
    }

    pub fn set_enabled_flag_only(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::SeqCst);
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
        if self
            .listeners_running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
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
        if let Ok(mut progress) = self.bench_progress.try_lock() {
            *progress = policy::BenchProgress::default();
        }
        if let Ok(mut retry) = self.loadout_retry_at.try_lock() {
            *retry = std::time::Instant::now();
        }
        if let Ok(mut retry) = self.bench_retry_at.try_lock() {
            *retry = std::time::Instant::now();
        }
        self.has_picked_champion.store(false, Ordering::SeqCst);
        self.has_banned_champion.store(false, Ordering::SeqCst);
        self.has_set_spells.store(false, Ordering::SeqCst);
        self.has_set_runes.store(false, Ordering::SeqCst);
        self.champ_select_version.fetch_add(1, Ordering::SeqCst);
    }

    async fn run_websocket_listener(&self, mut cancel: tokio::sync::watch::Receiver<bool>) {
        log::info!("[WS] WebSocket listener started, waiting for LCU...");

        // The WebSocket handshake validates LCU itself; optional plugin APIs must not gate startup.
        loop {
            if *cancel.borrow() {
                log::info!("[WS] WebSocket listener stopped");
                return;
            }

            // Get LCU auth (re-fetch each iteration — port/password can change on reconnect)
            let (port, password) = match self.riot_client.get_lcu_auth() {
                Some(auth) => auth,
                None => {
                    // LCU went away — wait for it to come back
                    log::info!("[WS] LCU disconnected, waiting for reconnect...");
                    self.riot_client.invalidate_cache();
                    self.ws_failures.store(0, Ordering::SeqCst);
                    self.force_polling.store(false, Ordering::SeqCst);
                    loop {
                        if let Some(auth) = self.riot_client.get_lcu_auth() {
                            log::info!("[WS] LCU credentials found (port={})", auth.0);
                            break auth;
                        }
                        tokio::select! {
                            _ = tokio::time::sleep(Duration::from_secs(3)) => {},
                            _ = cancel.changed() => return,
                        }
                    }
                }
            };

            // Build WebSocket URL
            let ws_url = self.riot_client.lcu_websocket_url(port);
            let auth =
                base64::engine::general_purpose::STANDARD.encode(format!("riot:{}", password));

            // Connect
            match self.ws_connect(&ws_url, &auth, &mut cancel).await {
                Ok(()) => {
                    self.riot_client.invalidate_cache();
                    self.force_polling.store(true, Ordering::SeqCst);
                }
                Err(e) => {
                    log::error!("[WS] WebSocket error: {}", e);
                    self.riot_client.invalidate_cache();
                    self.force_polling.store(true, Ordering::SeqCst);
                    let failures = self.ws_failures.fetch_add(1, Ordering::SeqCst) + 1;
                    if failures >= 3 {
                        self.force_polling.store(true, Ordering::SeqCst);
                        log::warn!(
                            "[WS] WebSocket failed {}x, enabling polling fallback",
                            failures
                        );
                    }
                }
            }

            if *cancel.borrow() {
                return;
            }
            // Cancel pending delayed actions without reapplying already completed loadouts on a WS-only outage.
            self.champ_select_version.fetch_add(1, Ordering::SeqCst);
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(2)) => {},
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
        let mut request = url
            .into_client_request()
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

        let (ws_stream, _) = tokio::select! {
            result = tokio::time::timeout(Duration::from_secs(10),
                tokio_tungstenite::connect_async_tls_with_config(request, None, false, Some(connector))) => {
                result.map_err(|_| AppError::Custom("WS connection timed out".into()))?
                    .map_err(|_| AppError::Custom("WS connection failed".into()))?
            },
            _ = cancel.changed() => return Ok(()),
        };

        let (mut write, mut read) = ws_stream.split();

        // Subscribe to events
        let sub_msg =
            tokio_tungstenite::tungstenite::Message::Text("[5, \"OnJsonApiEvent\"]".into());
        write
            .send(sub_msg)
            .await
            .map_err(|e| AppError::Custom(e.to_string()))?;

        log::info!("[WS] WebSocket connected to LCU, subscribed to OnJsonApiEvent");

        self.ws_failures.store(0, Ordering::SeqCst);
        self.force_polling.store(false, Ordering::SeqCst);
        // Subscriptions do not replay events missed while disconnected.
        tokio::select! {
            _ = self.refresh_lcu_snapshot() => {},
            _ = cancel.changed() => return Ok(()),
        }
        let mut heartbeat = tokio::time::interval(Duration::from_secs(10));
        heartbeat.tick().await;
        let mut last_message = std::time::Instant::now();
        // Read loop
        loop {
            tokio::select! {
                msg = read.next() => {
                    last_message = std::time::Instant::now();
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
                        Some(Ok(tokio_tungstenite::tungstenite::Message::Ping(data))) => {
                            write.send(tokio_tungstenite::tungstenite::Message::Pong(data)).await
                                .map_err(|_| AppError::Custom("WS heartbeat response failed".into()))?;
                        }
                        _ => {}
                    }
                }
                _ = heartbeat.tick() => {
                    if last_message.elapsed() > Duration::from_secs(30) {
                        return Err(AppError::Custom("WS heartbeat timed out".into()));
                    }
                    write.send(tokio_tungstenite::tungstenite::Message::Ping(Vec::new().into())).await
                        .map_err(|_| AppError::Custom("WS heartbeat failed".into()))?;
                }
                _ = cancel.changed() => {
                    log::info!("[WS] WebSocket cancelled");
                    return Ok(());
                }
            }
        }
    }

    async fn refresh_lcu_snapshot(&self) {
        if self.enabled.load(Ordering::SeqCst)
            && self.settings.read().await.auto_accept_method == "WebSocket"
        {
            if let Ok(body) = self
                .riot_client
                .lcu_get("/lol-matchmaking/v1/ready-check")
                .await
            {
                if let Ok(data) = serde_json::from_str(&body) {
                    self.handle_ready_check(&data).await;
                }
            }
        }
        if let Ok(body) = self
            .riot_client
            .lcu_get("/lol-champ-select/v1/session")
            .await
        {
            if let Ok(session) = serde_json::from_str(&body) {
                self.handle_champ_select_session(&session).await;
            }
        }
    }

    async fn handle_ws_message(&self, message: &str) {
        let Some(event) = lcu_protocol::api_event(message) else {
            return;
        };
        match event.uri.as_str() {
            "/lol-matchmaking/v1/ready-check" => {
                if event.event_type != "Delete"
                    && self.enabled.load(Ordering::SeqCst)
                    && self.settings.read().await.auto_accept_method == "WebSocket"
                {
                    self.handle_ready_check(&event.data).await;
                }
            }
            "/lol-champ-select/v1/session" => {
                if event.event_type == "Delete" {
                    self.champ_select_active.store(false, Ordering::SeqCst);
                    self.reset_champ_select_state();
                    if let Some(app) = self.app_handle.lock().await.as_ref() {
                        let _ = app.emit("automation-context", serde_json::Value::Null);
                    }
                } else {
                    self.handle_champ_select_session(&event.data).await;
                }
            }
            _ => {}
        }
    }

    async fn handle_ready_check(&self, data: &serde_json::Value) {
        if lcu_protocol::ready_timer(data).is_some() {
            self.accept_match().await;
        }
    }

    async fn accept_match(&self) -> bool {
        if !self.enabled.load(Ordering::SeqCst) {
            return false;
        }
        if self
            .accept_in_progress
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return false;
        }
        let attempt = AcceptAttempt(&self.accept_in_progress);

        let method = self.settings.read().await.auto_accept_method.clone();
        let ready = self
            .riot_client
            .lcu_get("/lol-matchmaking/v1/ready-check")
            .await
            .ok()
            .and_then(|body| serde_json::from_str::<serde_json::Value>(&body).ok())
            .and_then(|data| lcu_protocol::ready_timer(&data));
        if ready.is_none() || !self.enabled.load(Ordering::SeqCst) {
            return false;
        }
        let result = if method == "UIA" {
            #[cfg(windows)]
            {
                tokio::task::spawn_blocking(super::uia_login::accept_league_match)
                    .await
                    .map_err(|e| AppError::Custom(e.to_string()))
                    .and_then(|r| r.map_err(|e| AppError::Custom(e.to_string())))
            }
            #[cfg(not(windows))]
            {
                Err(AppError::Custom("UIA доступен только в Windows".into()))
            }
        } else {
            self.riot_client
                .lcu_post("/lol-matchmaking/v1/ready-check/accept", "{}")
                .await
                .map(|_| ())
        };

        drop(attempt);

        match result {
            Ok(_) => {
                log::info!("[AutoAccept] Match accepted automatically");
                self.emit_event("match-accepted", "Матч принят автоматически")
                    .await;
                true
            }
            Err(e) => {
                log::error!("[AutoAccept] Failed to accept match: {}", e);
                false
            }
        }
    }

    async fn current_bench_session(
        &self,
        previous: &serde_json::Value,
        mode: &str,
        id: i32,
    ) -> Option<(serde_json::Value, Vec<i32>, Vec<i32>)> {
        let started = std::time::Instant::now();
        let body = self
            .riot_client
            .lcu_get("/lol-gameflow/v1/session")
            .await
            .ok()?;
        let flow = serde_json::from_str::<serde_json::Value>(&body).ok()?;
        if policy::mode(&flow) != Some(mode) {
            return None;
        }
        let body = self
            .riot_client
            .lcu_get("/lol-champ-select/v1/pickable-champion-ids")
            .await
            .ok()?;
        let ids = serde_json::from_str::<Vec<i32>>(&body).ok()?;
        if !ids.contains(&id) {
            return None;
        }
        let subset = if previous["allowSubsetChampionPicks"] == true {
            self.subset_champions().await
        } else {
            vec![]
        };
        if previous["allowSubsetChampionPicks"] == true && !subset.contains(&id) {
            return None;
        }
        let body = self
            .riot_client
            .lcu_get("/lol-champ-select/v1/session")
            .await
            .ok()?;
        let fresh = serde_json::from_str::<serde_json::Value>(&body).ok()?;
        policy::bench_session_is_current(previous, &fresh, started.elapsed())
            .then_some((fresh, ids, subset))
    }

    async fn sync_loadout_profile(&self, mode: &str, role: &str) {
        let key = format!("{mode}:{role}");
        let mut previous = self.last_loadout_profile.lock().await;
        if *previous != key {
            *previous = key;
            self.has_set_spells.store(false, Ordering::SeqCst);
            self.has_set_runes.store(false, Ordering::SeqCst);
            *self.loadout_retry_at.lock().await = std::time::Instant::now();
        }
    }

    async fn handle_champ_select_session(&self, session: &serde_json::Value) {
        let Ok(_guard) = self.selection_lock.try_lock() else {
            return;
        };
        let Some(me) = policy::player(session) else {
            return;
        };
        let Ok(flow) = self.riot_client.lcu_get("/lol-gameflow/v1/session").await else {
            return;
        };
        let Ok(flow) = serde_json::from_str::<serde_json::Value>(&flow) else {
            return;
        };
        let Some(mode) = policy::mode(&flow) else {
            return;
        };
        let role = policy::role(me);
        if !self.champ_select_active.swap(true, Ordering::SeqCst) {
            self.reset_champ_select_state();
        }
        let settings_all = self.settings.read().await.clone();
        let settings = settings_all.for_queue(mode, role);
        let profile = settings_all.profiles.get(mode);

        let game_id = session.get("gameId").and_then(|g| g.as_i64()).unwrap_or(0);
        let prev_game_id = self.last_game_id.swap(game_id, Ordering::SeqCst);
        if game_id != prev_game_id && prev_game_id != 0 {
            log::info!(
                "[ChampSelect] New game session detected (gameId={}), resetting state",
                game_id
            );
            self.reset_champ_select_state();
        }

        let version = self.champ_select_version.load(Ordering::SeqCst);

        let my_cell = session
            .get("localPlayerCellId")
            .and_then(|c| c.as_i64())
            .unwrap_or(-1);
        if my_cell < 0 {
            return;
        }

        if let (Some(a), Some(b)) = (
            session["gameId"].as_i64(),
            flow["gameData"]["gameId"].as_i64(),
        ) {
            if a != b {
                return;
            }
        }
        if let Some(app) = self.app_handle.lock().await.as_ref() {
            let _ = app.emit(
                "automation-context",
                serde_json::json!({"mode":mode,"role":role,"roles":true}),
            );
        }
        self.sync_loadout_profile(mode, role).await;
        // Bound retries when the client rejects a restricted loadout.
        let mut retry = self.loadout_retry_at.lock().await;
        if std::time::Instant::now() >= *retry {
            *retry = std::time::Instant::now() + Duration::from_secs(5);
            if settings.auto_spells_enabled
                && !self.has_set_spells.load(Ordering::SeqCst)
                && self.set_summoner_spells(&settings, mode, role, me).await
            {
                self.has_set_spells.store(true, Ordering::SeqCst);
            }
            if settings.auto_runes_enabled
                && !self.has_set_runes.load(Ordering::SeqCst)
                && self.set_rune_page(&settings).await
            {
                self.has_set_runes.store(true, Ordering::SeqCst);
            }
        }
        drop(retry);
        if matches!(mode, "aram" | "mayhem") && settings.auto_pick_enabled && profile.is_some() {
            let mut progress = self.bench_progress.lock().await;
            progress.observe(session, std::time::Instant::now());
            let mut retry_at = self.bench_retry_at.lock().await;
            if std::time::Instant::now() >= *retry_at {
                if let Ok(body) = self
                    .riot_client
                    .lcu_get("/lol-champ-select/v1/pickable-champion-ids")
                    .await
                {
                    if let Ok(ids) = serde_json::from_str::<Vec<i32>>(&body) {
                        let subset = if session["allowSubsetChampionPicks"].as_bool() == Some(true)
                        {
                            self.subset_champions().await
                        } else {
                            Vec::new()
                        };
                        let coordinated = coordination::choice(session, mode);
                        let mut preferences = Self::bench_preferences(
                            session,
                            mode,
                            role,
                            profile.unwrap().active_pick_ids(),
                            &ids,
                            &subset,
                            profile.unwrap().bench,
                            &mut progress,
                        );
                        match coordinated {
                            Some(id) if id > 0 => preferences.retain(|pick| *pick == id),
                            None => preferences.clear(),
                            _ => {}
                        }
                        if let Some(id) = policy::bench_pick(
                            session,
                            &preferences,
                            &ids,
                            &subset,
                            profile.unwrap().bench,
                        ) {
                            if !progress.permits(session, id, std::time::Instant::now()) {
                                return;
                            }
                            *retry_at = std::time::Instant::now() + Duration::from_secs(3);
                            let swap_started = std::time::Instant::now();
                            let Some((fresh, ids, subset)) =
                                self.current_bench_session(session, mode, id).await
                            else {
                                return;
                            };
                            let current_settings = self.settings.read().await;
                            let enabled = current_settings.for_queue(mode, role).auto_pick_enabled;
                            let current_profile = current_settings.profiles.get(mode);
                            let still_preferred = enabled
                                && current_profile.is_some_and(|p| {
                                    let mut current_preferences = Self::bench_preferences(
                                        &fresh,
                                        mode,
                                        role,
                                        p.active_pick_ids(),
                                        &ids,
                                        &subset,
                                        p.bench,
                                        &mut progress,
                                    );
                                    if let Some(choice) = coordinated.filter(|id| *id > 0) {
                                        current_preferences.retain(|pick| *pick == choice);
                                    }
                                    policy::bench_pick(
                                        &fresh,
                                        &current_preferences,
                                        &ids,
                                        &subset,
                                        p.bench,
                                    ) == Some(id)
                                });
                            drop(current_settings);
                            if !still_preferred
                                || !policy::bench_session_is_current(
                                    session,
                                    &fresh,
                                    swap_started.elapsed(),
                                )
                                || !coordination::permits(&fresh, mode, id, false)
                            {
                                return;
                            }
                            progress.observe(&fresh, std::time::Instant::now());
                            if !progress.permits(&fresh, id, std::time::Instant::now()) {
                                return;
                            }
                            progress.attempt(id, fresh["timer"]["phase"] == "BAN_PICK");
                            match self
                                .riot_client
                                .lcu_post(
                                    &format!("/lol-champ-select/v1/session/bench/swap/{}", id),
                                    "{}",
                                )
                                .await
                            {
                                Ok(_) => log::info!(
                                    "[ChampSelect] {}: selected preferred bench champion {}",
                                    mode,
                                    id
                                ),
                                Err(e) => log::warn!(
                                    "[ChampSelect] Bench unavailable, keeping current champion: {}",
                                    e
                                ),
                            }
                        }
                    }
                }
            }
        }

        // Collect banned champion IDs
        let mut banned_ids: Vec<i64> = Vec::new();
        if let Some(actions) = session.get("actions").and_then(|a| a.as_array()) {
            for action_group in actions {
                if let Some(group) = action_group.as_array() {
                    for action in group {
                        let action_type = action.get("type").and_then(|t| t.as_str()).unwrap_or("");
                        let completed = action
                            .get("completed")
                            .and_then(|c| c.as_bool())
                            .unwrap_or(false);
                        let champ_id = action
                            .get("championId")
                            .and_then(|c| c.as_i64())
                            .unwrap_or(0);
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
                        let actor = action
                            .get("actorCellId")
                            .and_then(|a| a.as_i64())
                            .unwrap_or(-1);
                        if actor == my_cell {
                            let action_type =
                                action.get("type").and_then(|t| t.as_str()).unwrap_or("?");
                            let completed = action
                                .get("completed")
                                .and_then(|c| c.as_bool())
                                .unwrap_or(false);
                            let is_in_progress = action
                                .get("isInProgress")
                                .and_then(|i| i.as_bool())
                                .unwrap_or(false);
                            let action_id = action.get("id").and_then(|i| i.as_i64()).unwrap_or(0);
                            let champ_id = action
                                .get("championId")
                                .and_then(|c| c.as_i64())
                                .unwrap_or(0);
                            my_actions_summary.push(format!(
                                "{}(id={},champ={},prog={},done={})",
                                action_type, action_id, champ_id, is_in_progress, completed
                            ));
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
                        let actor = action
                            .get("actorCellId")
                            .and_then(|a| a.as_i64())
                            .unwrap_or(-1);
                        if actor != my_cell {
                            continue;
                        }

                        let action_type = action.get("type").and_then(|t| t.as_str()).unwrap_or("");
                        let completed = action
                            .get("completed")
                            .and_then(|c| c.as_bool())
                            .unwrap_or(false);
                        let action_id = action.get("id").and_then(|i| i.as_i64()).unwrap_or(0);
                        let is_in_progress = action
                            .get("isInProgress")
                            .and_then(|i| i.as_bool())
                            .unwrap_or(false);

                        if completed {
                            continue;
                        }

                        if version != self.champ_select_version.load(Ordering::SeqCst) {
                            return;
                        }

                        if action_type == "ban" && is_in_progress && settings.auto_ban_enabled {
                            if self
                                .has_banned_champion
                                .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                                .is_ok()
                            {
                                log::info!(
                                    "[ChampSelect] Ban phase, my turn (actionId={})",
                                    action_id
                                );
                                let bannable = match self
                                    .riot_client
                                    .lcu_get("/lol-champ-select/v1/bannable-champion-ids")
                                    .await
                                {
                                    Ok(body) => {
                                        serde_json::from_str::<Vec<i32>>(&body).unwrap_or_default()
                                    }
                                    Err(_) => Vec::new(),
                                };
                                let available = Self::available_action_pool(
                                    &session,
                                    &bannable,
                                    &bannable,
                                    &[],
                                    true,
                                );
                                let selected = self
                                    .select_ban(
                                        &session, mode, role, action_id, game_id, &available,
                                    )
                                    .await;
                                let success = if let Some(id) = selected {
                                    self.do_champion_action(action_id, id, true, game_id, role)
                                        .await
                                } else {
                                    log::info!(
                                        "[ChampSelect] No eligible ban; leaving manual selection"
                                    );
                                    false
                                };
                                if !success {
                                    self.has_banned_champion.store(false, Ordering::SeqCst);
                                }
                            }
                        }

                        if action_type == "pick"
                            && is_in_progress
                            && settings.auto_pick_enabled
                            && (mode != "arena"
                                || (policy::bans_finished(session)
                                    && policy::arena_team_known(
                                        session,
                                        flow["gameData"]["queue"]["numPlayersPerTeam"].as_u64(),
                                    )))
                            && !(mode == "arena"
                                && profile.is_some_and(|p| p.arena_pick == "manual"))
                        {
                            if self
                                .has_picked_champion
                                .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                                .is_ok()
                            {
                                log::info!(
                                    "[ChampSelect] Pick phase, my turn (actionId={})",
                                    action_id
                                );

                                // Get pickable champion IDs to check availability
                                let mut pickable_ids = match self
                                    .riot_client
                                    .lcu_get("/lol-champ-select/v1/pickable-champion-ids")
                                    .await
                                {
                                    Ok(body) => {
                                        match serde_json::from_str::<Vec<i64>>(&body) {
                                            Ok(ids) => {
                                                log::info!(
                                                    "[ChampSelect] {} pickable champions available",
                                                    ids.len()
                                                );
                                                ids
                                            }
                                            Err(_) => {
                                                log::warn!("[ChampSelect] Invalid pickable champions response");
                                                Vec::new()
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        log::warn!(
                                            "[ChampSelect] Failed to get pickable champions: {}",
                                            e
                                        );
                                        Vec::new()
                                    }
                                };

                                if session["allowSubsetChampionPicks"].as_bool() == Some(true) {
                                    let subset = self.subset_champions().await;
                                    pickable_ids.retain(|id| subset.contains(&(*id as i32)));
                                }
                                let mut settings = settings.clone();
                                if mode == "arena"
                                    && profile.is_some_and(|p| p.arena_pick == "crowd")
                                {
                                    let favorites = match self.riot_client.lcu_get("/lol-lobby-team-builder/champ-select/v1/crowd-favorte-champion-list").await {
                                        Ok(body) => serde_json::from_str::<Vec<i32>>(&body).unwrap_or_default(),
                                        Err(_) => Vec::new(),
                                    };
                                    let preferred = [
                                        settings.pick_champion1_id,
                                        settings.pick_champion2_id,
                                        settings.pick_champion3_id,
                                    ];
                                    let mut pool: Vec<_> = preferred
                                        .into_iter()
                                        .flatten()
                                        .chain(favorites.iter().copied())
                                        .collect();
                                    let mut seen = std::collections::BTreeSet::new();
                                    pool.retain(|id| seen.insert(*id));
                                    let choice = pool.into_iter().find(|id| {
                                        let mut declared: Vec<_> =
                                            preferred.iter().flatten().copied().collect();
                                        declared.push(*id);
                                        let mut seen = std::collections::BTreeSet::new();
                                        declared.retain(|id| seen.insert(*id));
                                        coordination::local_preference_ids(
                                            mode,
                                            role,
                                            &declared,
                                            false,
                                            &Self::available_action_pool(
                                                session,
                                                &declared,
                                                &favorites
                                                    .iter()
                                                    .copied()
                                                    .filter(|id| {
                                                        pickable_ids.contains(&(*id as i64))
                                                    })
                                                    .collect::<Vec<_>>(),
                                                &pickable_ids
                                                    .iter()
                                                    .map(|id| *id as i32)
                                                    .collect::<Vec<_>>(),
                                                false,
                                            ),
                                        )
                                        .contains(id)
                                            && *id > 0
                                            && favorites.contains(id)
                                            && pickable_ids.contains(&(*id as i64))
                                            && !banned_ids.contains(&(*id as i64))
                                    });
                                    settings.pick_champion1 =
                                        choice.map(|id| format!("Crowd Favorite {}", id));
                                    settings.pick_champion1_id = choice;
                                    settings.pick_champion2 = None;
                                    settings.pick_champion3 = None;
                                }
                                let mut picks = vec![
                                    (settings.pick_champion1.clone(), settings.pick_champion1_id),
                                    (settings.pick_champion2.clone(), settings.pick_champion2_id),
                                    (settings.pick_champion3.clone(), settings.pick_champion3_id),
                                ];

                                if matches!(mode, "aram" | "mayhem") {
                                    picks.clear();
                                    if session["allowSubsetChampionPicks"] == true
                                        && session["timer"]["phase"] == "BAN_PICK"
                                    {
                                        let subset = self.subset_champions().await;
                                        let ids: Vec<i32> = pickable_ids
                                            .iter()
                                            .filter_map(|id| i32::try_from(*id).ok())
                                            .collect();
                                        let mut progress = self.bench_progress.lock().await;
                                        let preferred = Self::bench_preferences(
                                            session,
                                            mode,
                                            role,
                                            profile.unwrap().active_pick_ids(),
                                            &ids,
                                            &subset,
                                            false,
                                            &mut progress,
                                        );
                                        picks = preferred
                                            .into_iter()
                                            .map(|id| {
                                                (Some(format!("Personal card {}", id)), Some(id))
                                            })
                                            .collect();
                                    }
                                }
                                let pool: Vec<_> = picks.iter().filter_map(|(_, id)| *id).collect();
                                let allowed = coordination::local_preference_ids(
                                    mode,
                                    role,
                                    &pool,
                                    false,
                                    &Self::available_action_pool(
                                        session,
                                        &pool,
                                        &pickable_ids
                                            .iter()
                                            .map(|id| *id as i32)
                                            .collect::<Vec<_>>(),
                                        &pickable_ids
                                            .iter()
                                            .map(|id| *id as i32)
                                            .collect::<Vec<_>>(),
                                        false,
                                    ),
                                );
                                picks.retain(|(_, id)| id.is_some_and(|id| allowed.contains(&id)));
                                let mut picked = false;
                                match coordination::choice(session, mode) {
                                    Some(id) if id > 0 => {
                                        if picks.iter().any(|(_, pick)| *pick == Some(id)) {
                                            picks = vec![(Some(format!("Party {}", id)), Some(id))];
                                        } else {
                                            picks.clear();
                                        }
                                    }
                                    None => picks.clear(),
                                    _ => {}
                                }
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
                                            if !pickable_ids.contains(&pick_id) {
                                                log::warn!("[ChampSelect] Pick '{}' (id={}) is NOT in pickable list (not owned/free rotation?), trying next", name, pick_id);
                                                continue;
                                            }
                                            log::info!(
                                                "[ChampSelect] Picking '{}' (id={})",
                                                name,
                                                pick_id
                                            );
                                            let success = self
                                                .do_champion_action(
                                                    action_id,
                                                    pick_id as i32,
                                                    false,
                                                    game_id,
                                                    role,
                                                )
                                                .await;
                                            if success {
                                                picked = true;
                                                break;
                                            }
                                            // If this pick failed, try next champion
                                            log::warn!(
                                                "[ChampSelect] Pick '{}' failed, trying next",
                                                name
                                            );
                                        }
                                    }
                                }
                                if !picked {
                                    // Reset flag so next WS event can retry
                                    self.has_picked_champion.store(false, Ordering::SeqCst);
                                    log::warn!(
                                        "[ChampSelect] No pick succeeded, will retry on next event"
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    async fn subset_champions(&self) -> Vec<i32> {
        match self
            .riot_client
            .lcu_get("/lol-lobby-team-builder/champ-select/v1/subset-champion-list")
            .await
        {
            Ok(body) => serde_json::from_str(&body).unwrap_or_default(),
            Err(_) => Vec::new(),
        }
    }

    fn has_configured_ban_intent(settings: &AutomationSettings, mode: &str, role: &str) -> bool {
        let key = if mode == "classic" {
            format!("classic:{role}")
        } else {
            mode.into()
        };
        settings.profiles.get(&key).is_some_and(|profile| {
            // An empty ban pool is not consent to random bans; inspect IDs before own-pick filtering.
            if profile.bans.is_empty() {
                profile.ban_id.is_some_and(|id| (1..=10000).contains(&id))
            } else {
                profile
                    .ban_ids
                    .iter()
                    .take(3)
                    .any(|id| (1..=10000).contains(id))
            }
        })
    }

    async fn select_ban(
        &self,
        session: &Value,
        mode: &str,
        role: &str,
        action_id: i64,
        game_id: i64,
        available: &[i32],
    ) -> Option<i32> {
        let settings = self.settings.read().await;
        let effective = settings.for_queue(mode, role);
        if !effective.auto_ban_enabled || !Self::has_configured_ban_intent(&settings, mode, role) {
            return None;
        }
        // Preserve the existing stronger protection for every configured own pick.
        let own_picks = [
            effective.pick_champion1_id,
            effective.pick_champion2_id,
            effective.pick_champion3_id,
        ];
        let safe: Vec<_> = available
            .iter()
            .copied()
            .filter(|id| {
                !own_picks.contains(&Some(*id)) && coordination::ban_is_safe(session, mode, *id)
            })
            .collect();
        let configured = Self::configured_action_pool(&settings, mode, role, true);
        let cell = session["localPlayerCellId"].as_i64()?;
        self.ban_fallback.lock().await.choose(
            (game_id, action_id, cell),
            &configured,
            available,
            &safe,
        )
    }

    /// Re-read current settings and local preferences even when group coordination is inactive.
    async fn local_action_is_preferred(
        &self,
        session: &Value,
        mode: &str,
        role: &str,
        champion_id: i32,
        is_ban: bool,
        available: &[i32],
    ) -> bool {
        if !is_ban && matches!(mode, "aram" | "mayhem") {
            let mut progress = self.bench_progress.lock().await;
            progress.observe(session, std::time::Instant::now());
            let settings = self.settings.read().await;
            let Some(profile) = settings.profiles.get(mode) else {
                return false;
            };
            let mut pool = profile.active_pick_ids().to_vec();
            pool.extend(
                available
                    .iter()
                    .filter(|id| !profile.active_pick_ids().contains(id))
                    .copied(),
            );
            let eligible = coordination::local_preference_ids(mode, role, &pool, false, available);
            return session["allowSubsetChampionPicks"] == true
                && session["timer"]["phase"] == "BAN_PICK"
                && progress.personal_matches(champion_id, &eligible)
                && settings.for_queue(mode, role).auto_pick_enabled;
        }
        let crowd = !is_ban
            && mode == "arena"
            && self
                .settings
                .read()
                .await
                .profiles
                .get(mode)
                .is_some_and(|p| p.arena_pick == "crowd");
        let favorites = if crowd {
            match self
                .riot_client
                .lcu_get("/lol-lobby-team-builder/champ-select/v1/crowd-favorte-champion-list")
                .await
            {
                Ok(body) => serde_json::from_str::<Vec<i32>>(&body).unwrap_or_default(),
                Err(_) => return false,
            }
        } else {
            Vec::new()
        };
        let settings = self.settings.read().await;
        let effective = settings.for_queue(mode, role);
        if if is_ban {
            !effective.auto_ban_enabled
        } else {
            !effective.auto_pick_enabled
        } {
            return false;
        }
        let mut pool = Self::configured_action_pool(&settings, mode, role, is_ban);
        if !is_ban
            && mode == "arena"
            && settings
                .profiles
                .get(mode)
                .is_some_and(|p| p.arena_pick == "crowd")
        {
            if !favorites.contains(&champion_id) {
                return false;
            }
            pool.push(champion_id);
            let mut seen = std::collections::BTreeSet::new();
            pool.retain(|id| seen.insert(*id));
        }
        let preferred = coordination::local_preference_ids(
            mode,
            role,
            &pool,
            is_ban,
            &available
                .iter()
                .copied()
                .filter(|id| !crowd || favorites.contains(id))
                .collect::<Vec<_>>(),
        );
        pool.contains(&champion_id)
            && if is_ban {
                preferred.first() == Some(&champion_id)
            } else {
                preferred.contains(&champion_id)
            }
    }

    fn bench_preferences(
        session: &Value,
        mode: &str,
        role: &str,
        pool: &[i32],
        ids: &[i32],
        subset: &[i32],
        shared: bool,
        progress: &mut policy::BenchProgress,
    ) -> Vec<i32> {
        if session["allowSubsetChampionPicks"] == true && session["timer"]["phase"] == "BAN_PICK" {
            let physical: Vec<_> = subset
                .iter()
                .copied()
                .filter(|id| ids.contains(id))
                .collect();
            let mut declared = pool.to_vec();
            declared.extend(physical.iter().filter(|id| !pool.contains(id)).copied());
            let eligible =
                coordination::local_preference_ids(mode, role, &declared, false, &physical);
            let preferred = coordination::local_preference_ids(mode, role, pool, false, &physical);
            return progress
                .personal_choice(&preferred, &eligible)
                .into_iter()
                .collect();
        }
        coordination::local_preference_ids(
            mode,
            role,
            pool,
            false,
            &Self::available_bench_pool(session, pool, ids, subset, shared),
        )
    }

    fn available_bench_pool(
        session: &Value,
        pool: &[i32],
        ids: &[i32],
        subset: &[i32],
        shared: bool,
    ) -> Vec<i32> {
        let current = policy::player(session).and_then(|p| p["championId"].as_i64());
        pool.iter()
            .copied()
            .filter(|id| {
                current == Some(*id as i64)
                    || policy::bench_pick(session, &[*id], ids, subset, shared) == Some(*id)
            })
            .collect()
    }

    fn available_action_pool(
        session: &Value,
        pool: &[i32],
        ids: &[i32],
        subset: &[i32],
        ban: bool,
    ) -> Vec<i32> {
        pool.iter()
            .copied()
            .filter(|id| {
                *id > 0
                    && ids.contains(id)
                    && !["myTeamBans", "theirTeamBans"].iter().any(|key| {
                        session["bans"][*key]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .any(|b| b.as_i64() == Some(*id as i64))
                    })
                    && (ban || session["allowSubsetChampionPicks"] != true || subset.contains(id))
                    && !session["actions"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_array)
                        .flatten()
                        .any(|a| {
                            (a["type"] == "ban" || ban && a["type"] == "pick")
                                && a["completed"] == true
                                && a["championId"].as_i64() == Some(*id as i64)
                        })
                    && !(ban
                        && session["theirTeam"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .any(|p| p["championId"].as_i64() == Some(*id as i64)))
                    && !session["myTeam"].as_array().into_iter().flatten().any(|p| {
                        (ban || p["cellId"] != session["localPlayerCellId"])
                            && (ban || policy::same_pick_team(session, p))
                            && (p["championId"].as_i64() == Some(*id as i64)
                                || ban && p["championPickIntent"].as_i64() == Some(*id as i64))
                    })
            })
            .collect()
    }

    fn configured_action_pool(
        settings: &AutomationSettings,
        mode: &str,
        role: &str,
        is_ban: bool,
    ) -> Vec<i32> {
        let effective = settings.for_queue(mode, role);
        if is_ban {
            return if effective.auto_ban_enabled {
                effective.ban_champion_ids.clone()
            } else {
                vec![]
            };
        }
        if !effective.auto_pick_enabled
            || mode == "arena"
                && settings
                    .profiles
                    .get(mode)
                    .is_some_and(|p| p.arena_pick == "manual")
        {
            return vec![];
        }
        if matches!(mode, "aram" | "mayhem") {
            return settings
                .profiles
                .get(mode)
                .map(|p| p.active_pick_ids().to_vec())
                .unwrap_or_default();
        }
        [
            effective.pick_champion1_id,
            effective.pick_champion2_id,
            effective.pick_champion3_id,
        ]
        .into_iter()
        .flatten()
        .collect()
    }

    /// Re-read live availability and session before each coordinated mutation.
    async fn coordinated_action_is_current(
        &self,
        mode: &str,
        champion_id: i32,
        is_ban: bool,
        action_id: i64,
        game_id: i64,
        role: &str,
    ) -> bool {
        let started = std::time::Instant::now();
        let path = if is_ban {
            "/lol-champ-select/v1/bannable-champion-ids"
        } else {
            "/lol-champ-select/v1/pickable-champion-ids"
        };
        let Ok(ids) = self.riot_client.lcu_get(path).await else {
            return false;
        };
        let Ok(ids) = serde_json::from_str::<Vec<i32>>(&ids) else {
            return false;
        };
        if !ids.contains(&champion_id) {
            return false;
        }
        let subset = if is_ban {
            Vec::new()
        } else {
            self.subset_champions().await
        };
        let Ok(body) = self
            .riot_client
            .lcu_get("/lol-champ-select/v1/session")
            .await
        else {
            return false;
        };
        let Ok(session) = serde_json::from_str::<serde_json::Value>(&body) else {
            return false;
        };
        if !is_ban && session["allowSubsetChampionPicks"] == true && !subset.contains(&champion_id)
        {
            return false;
        }
        if started.elapsed() >= Duration::from_secs(3)
            || !session["timer"]["adjustedTimeLeftInPhase"]
                .as_f64()
                .is_some_and(|ms| ms > started.elapsed().as_secs_f64() * 1000.0 + 250.0)
        {
            return false;
        }
        if !is_ban && mode == "arena" {
            let Ok(body) = self.riot_client.lcu_get("/lol-gameflow/v1/session").await else {
                return false;
            };
            let Ok(flow) = serde_json::from_str::<Value>(&body) else {
                return false;
            };
            if policy::mode(&flow) != Some("arena")
                || !policy::bans_finished(&session)
                || !policy::arena_team_known(
                    &session,
                    flow["gameData"]["queue"]["numPlayersPerTeam"].as_u64(),
                )
            {
                return false;
            }
        }
        let available = Self::available_action_pool(&session, &ids, &ids, &subset, is_ban);
        if is_ban {
            if self
                .select_ban(&session, mode, role, action_id, game_id, &available)
                .await
                != Some(champion_id)
            {
                return false;
            }
        } else if !self
            .local_action_is_preferred(&session, mode, role, champion_id, false, &available)
            .await
        {
            return false;
        }
        started.elapsed() < Duration::from_secs(3)
            && session["timer"]["adjustedTimeLeftInPhase"]
                .as_f64()
                .is_some_and(|ms| ms > started.elapsed().as_secs_f64() * 1000.0 + 250.0)
            && policy::action_is_current(&session, action_id, game_id, role, is_ban, champion_id)
            && coordination::permits(&session, mode, champion_id, is_ban)
    }

    /// Perform one champion action; callers reset their flag after a failed attempt.
    async fn do_champion_action(
        &self,
        action_id: i64,
        champion_id: i32,
        is_ban: bool,
        game_id: i64,
        role: &str,
    ) -> bool {
        let started = std::time::Instant::now();
        let verified = self
            .do_champion_action_inner(action_id, champion_id, is_ban, game_id, role)
            .await;
        self.riot_client
            .diagnostics
            .champion_result(verified, started.elapsed());
        verified
    }

    async fn do_champion_action_inner(
        &self,
        action_id: i64,
        champion_id: i32,
        is_ban: bool,
        game_id: i64,
        role: &str,
    ) -> bool {
        let Ok(body) = self
            .riot_client
            .lcu_get("/lol-champ-select/v1/session")
            .await
        else {
            return false;
        };
        let Ok(session) = serde_json::from_str::<serde_json::Value>(&body) else {
            return false;
        };
        if !policy::action_is_current(&session, action_id, game_id, role, is_ban, champion_id) {
            return false;
        }
        let Ok(flow) = self.riot_client.lcu_get("/lol-gameflow/v1/session").await else {
            return false;
        };
        let Ok(flow) = serde_json::from_str::<serde_json::Value>(&flow) else {
            return false;
        };
        let Some(mode) = policy::mode(&flow) else {
            return false;
        };
        if !coordination::permits(&session, mode, champion_id, is_ban) {
            return false;
        }
        if !is_ban && session["allowSubsetChampionPicks"].as_bool() == Some(true) {
            if !self.subset_champions().await.contains(&champion_id) {
                return false;
            }
            let endpoint = format!("/lol-champ-select/v1/session/actions/{}", action_id);
            let body = serde_json::json!({"championId":champion_id,"type":"pick","completed":true});
            if !self
                .coordinated_action_is_current(mode, champion_id, is_ban, action_id, game_id, role)
                .await
            {
                return false;
            }
            if matches!(mode, "aram" | "mayhem") {
                let mut progress = self.bench_progress.lock().await;
                progress.observe(&session, std::time::Instant::now());
                if !progress.permits(&session, champion_id, std::time::Instant::now()) {
                    return false;
                }
                progress.attempt(champion_id, true);
            }
            if self
                .riot_client
                .lcu_patch(&endpoint, &body.to_string())
                .await
                .is_err()
            {
                return false;
            }
            tokio::time::sleep(Duration::from_millis(300)).await;
            return match self
                .riot_client
                .lcu_get("/lol-champ-select/v1/session")
                .await
            {
                Ok(body) => serde_json::from_str::<serde_json::Value>(&body)
                    .ok()
                    .is_some_and(|s| {
                        s["gameId"].as_i64() == Some(game_id)
                            && Self::check_action_champion(&s, action_id, champion_id as i64)
                            && Self::check_action_completed(&s, action_id)
                    }),
                Err(_) => false,
            };
        }
        let action_str = if is_ban { "ban" } else { "pick" };
        let endpoint = format!("/lol-champ-select/v1/session/actions/{}", action_id);

        // Step 1: PATCH to hover (set championId without completing)
        let hover_body = format!("{{\"championId\":{}}}", champion_id);
        log::info!(
            "[ChampSelect] {} step1 hover: PATCH {} body={}",
            action_str,
            endpoint,
            hover_body
        );

        if !self
            .coordinated_action_is_current(mode, champion_id, is_ban, action_id, game_id, role)
            .await
        {
            return false;
        }
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
        let hover_verified = match self
            .riot_client
            .lcu_get("/lol-champ-select/v1/session")
            .await
        {
            Ok(session_str) => {
                if let Ok(session) = serde_json::from_str::<serde_json::Value>(&session_str) {
                    policy::action_is_current(
                        &session,
                        action_id,
                        game_id,
                        role,
                        is_ban,
                        champion_id,
                    ) && coordination::permits(&session, mode, champion_id, is_ban)
                        && Self::check_action_champion(&session, action_id, champion_id as i64)
                } else {
                    false
                }
            }
            Err(e) => {
                log::warn!("[ChampSelect] {} verify GET failed: {}", action_str, e);
                false // Never lock an action whose current state could not be verified.
            }
        };

        if !hover_verified {
            log::warn!(
                "[ChampSelect] {} hover NOT verified, will retry on next event",
                action_str
            );
            return false;
        }

        log::info!(
            "[ChampSelect] {} hover verified, proceeding to lock-in",
            action_str
        );

        // Step 2: PATCH with completed=true to lock in
        let complete_body = format!("{{\"championId\":{},\"completed\":true}}", champion_id);
        log::info!(
            "[ChampSelect] {} step2 lock-in: PATCH {} body={}",
            action_str,
            endpoint,
            complete_body
        );

        if !self
            .coordinated_action_is_current(mode, champion_id, is_ban, action_id, game_id, role)
            .await
        {
            return false;
        }
        match self.riot_client.lcu_patch(&endpoint, &complete_body).await {
            Ok(resp) => log::info!("[ChampSelect] {} lock-in ok: '{}'", action_str, resp),
            Err(e) => {
                log::error!("[ChampSelect] {} lock-in FAILED: {}", action_str, e);
                return false;
            }
        }

        // Verify completion
        tokio::time::sleep(Duration::from_millis(300)).await;

        match self
            .riot_client
            .lcu_get("/lol-champ-select/v1/session")
            .await
        {
            Ok(session_str) => {
                if let Ok(session) = serde_json::from_str::<serde_json::Value>(&session_str) {
                    if Self::check_action_completed(&session, action_id) {
                        log::info!("[ChampSelect] {} VERIFIED completed!", action_str);
                        return true;
                    }
                    log::warn!(
                        "[ChampSelect] {} NOT completed after lock-in, will retry on next event",
                        action_str
                    );
                }
            }
            Err(e) => log::warn!(
                "[ChampSelect] {} completion verify failed: {}",
                action_str,
                e
            ),
        }

        false
    }

    /// Check if an action has the expected championId set
    fn check_action_champion(
        session: &serde_json::Value,
        action_id: i64,
        expected_champ: i64,
    ) -> bool {
        if let Some(actions) = session.get("actions").and_then(|a| a.as_array()) {
            for group in actions {
                if let Some(arr) = group.as_array() {
                    for action in arr {
                        let id = action.get("id").and_then(|i| i.as_i64()).unwrap_or(-1);
                        if id == action_id {
                            let champ = action
                                .get("championId")
                                .and_then(|c| c.as_i64())
                                .unwrap_or(0);
                            log::info!(
                                "[ChampSelect] verify action {}: championId={} (expected={})",
                                action_id,
                                champ,
                                expected_champ
                            );
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
                            let completed = action
                                .get("completed")
                                .and_then(|c| c.as_bool())
                                .unwrap_or(false);
                            let champ = action
                                .get("championId")
                                .and_then(|c| c.as_i64())
                                .unwrap_or(0);
                            log::info!(
                                "[ChampSelect] verify action {}: championId={}, completed={}",
                                action_id,
                                champ,
                                completed
                            );
                            return completed;
                        }
                    }
                }
            }
        }
        false
    }

    async fn set_rune_page(&self, settings: &AutomationSettings) -> bool {
        if settings.selected_rune_page_name.is_empty() {
            return true;
        }

        // Find the rune page by name from local storage
        let pages = self.rune_pages_storage.load_all();
        let page = match pages
            .iter()
            .find(|p| p.name == settings.selected_rune_page_name)
        {
            Some(p) => p,
            None => {
                log::warn!(
                    "[ChampSelect] Rune page '{}' not found",
                    settings.selected_rune_page_name
                );
                return false;
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
                return false;
            }
        };

        // Only overwrite the page managed by RustLM.
        let mut rustlm_page_id: Option<i64> = None;

        if let Some(arr) = lcu_pages.as_array() {
            for p in arr {
                let name = p.get("name").and_then(|n| n.as_str()).unwrap_or("");
                let id = p.get("id").and_then(|i| i.as_i64()).unwrap_or(0);
                if name == "RustLM" {
                    rustlm_page_id = Some(id);
                    break;
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
                Ok(_) => true,
                Err(e) => {
                    log::error!("[ChampSelect] Failed to update rune page: {}", e);
                    false
                }
            }
        } else {
            // Preserve the player's pages; the client may reject creation if all slots are full.
            match self
                .riot_client
                .lcu_post("/lol-perks/v1/pages", &body.to_string())
                .await
            {
                Ok(_) => true,
                Err(e) => {
                    log::error!("[ChampSelect] Failed to create rune page: {}", e);
                    false
                }
            }
        }
    }

    async fn set_summoner_spells(
        &self,
        settings: &AutomationSettings,
        mode: &str,
        role: &str,
        me: &serde_json::Value,
    ) -> bool {
        let spell1 = settings
            .spell1_id
            .filter(|id| policy::spell_allowed(mode, *id));
        let spell2 = settings
            .spell2_id
            .filter(|id| policy::spell_allowed(mode, *id));
        if spell1.is_none() && spell2.is_none() {
            return true;
        }
        let desired1 = spell1.map(i64::from).or_else(|| me["spell1Id"].as_i64());
        let desired2 = spell2.map(i64::from).or_else(|| me["spell2Id"].as_i64());
        if desired1 == desired2 || desired1.is_none() || desired2.is_none() {
            return false;
        }
        // Respect queue/position restrictions supplied by the running client, including required Smite.
        for (kind, required) in [("disallowed", false), ("required", true)] {
            let Ok(body) = self
                .riot_client
                .lcu_get(&format!(
                    "/lol-gameflow/v1/session/per-position-summoner-spells/{}",
                    kind
                ))
                .await
            else {
                return false;
            };
            let Ok(rules) = serde_json::from_str::<serde_json::Value>(&body) else {
                return false;
            };
            let Some(rules) = rules.as_object() else {
                return false;
            };
            for (position, list) in rules {
                if !position.is_empty()
                    && position != "*"
                    && position.to_lowercase() != role
                    && position != "UNSELECTED"
                {
                    continue;
                }
                let Some(spells) = list["spells"].as_array() else {
                    return false;
                };
                for spell in spells {
                    let Some(id) = spell.as_i64() else {
                        return false;
                    };
                    let contains = desired1 == Some(id) || desired2 == Some(id);
                    if (required && !contains) || (!required && contains) {
                        return false;
                    }
                }
            }
        }
        let mut body = serde_json::Map::new();
        if let Some(id) = spell1 {
            body.insert("spell1Id".into(), serde_json::json!(id));
        }
        if let Some(id) = spell2 {
            body.insert("spell2Id".into(), serde_json::json!(id));
        }
        match self
            .riot_client
            .lcu_patch(
                "/lol-champ-select/v1/session/my-selection",
                &serde_json::Value::Object(body).to_string(),
            )
            .await
        {
            Ok(_) => true,
            Err(e) => {
                log::warn!(
                    "[ChampSelect] Cannot set {} spells, keeping current loadout: {}",
                    mode,
                    e
                );
                false
            }
        }
    }

    async fn run_polling_listener(&self, mut cancel: tokio::sync::watch::Receiver<bool>) {
        log::info!("[Polling] Polling listener started, waiting for LCU...");

        // Wait for LCU to become available before polling
        loop {
            if *cancel.borrow() {
                log::info!("[Polling] Polling listener stopped (cancelled while waiting for LCU)");
                return;
            }
            if let Some(auth) = self.riot_client.get_lcu_auth() {
                // Ready-check requests validate their own availability and HTTP status.
                log::info!(
                    "[Polling] LCU credentials found (port={}), starting polling",
                    auth.0
                );
                break;
            }
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(3)) => {},
                _ = cancel.changed() => return,
            }
        }

        let mut last_lcu_ok = true;

        loop {
            if *cancel.borrow() {
                log::info!("[Polling] Polling listener stopped");
                return;
            }

            // Check LCU connectivity
            let lcu_auth = self.riot_client.get_lcu_auth();
            if lcu_auth.is_none() {
                if last_lcu_ok {
                    log::warn!("[Polling] LCU disconnected. Waiting for reconnect...");
                    last_lcu_ok = false;
                    self.riot_client.invalidate_cache();
                }
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(3)) => continue,
                    _ = cancel.changed() => return,
                }
            }

            if !last_lcu_ok {
                log::info!("[Polling] LCU reconnected, polling active");
                last_lcu_ok = true;
            }

            // Poll ready-check (only when auto-accept is enabled)
            if self.enabled.load(Ordering::SeqCst) {
                match self
                    .riot_client
                    .lcu_get("/lol-matchmaking/v1/ready-check")
                    .await
                {
                    Ok(body) => {
                        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body) {
                            self.handle_ready_check(&json).await;
                        }
                    }
                    Err(_) => {} // Expected when not in queue
                }
            }

            // Poll champ-select session (always active for auto-pick/ban/spells/runes)
            match self
                .riot_client
                .lcu_get("/lol-champ-select/v1/session")
                .await
            {
                Ok(body) => {
                    if let Ok(session) = serde_json::from_str::<serde_json::Value>(&body) {
                        self.handle_champ_select_session(&session).await;
                    }
                }
                Err(_) => {
                    if self.champ_select_active.swap(false, Ordering::SeqCst) {
                        self.reset_champ_select_state();
                        if let Some(app) = self.app_handle.lock().await.as_ref() {
                            let _ = app.emit("automation-context", serde_json::Value::Null);
                        }
                    }
                }
            }

            // Faster polling when WS is down
            let delay = if self.champ_select_active.load(Ordering::SeqCst)
                || self.settings.read().await.auto_accept_method != "WebSocket"
                || self.force_polling.load(Ordering::SeqCst)
            {
                300
            } else {
                5000
            };
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_millis(delay)) => {},
                _ = cancel.changed() => return,
            }
        }
    }
}

#[cfg(all(test, debug_assertions))]
mod simulator_tests {
    use super::*;
    use crate::models::automation::AutomationProfile;

    #[test]
    fn current_action_pool_tracks_assigned_role_toggles_and_replaced_ids() {
        let mut settings = AutomationSettings::default();
        settings.auto_pick_enabled = true;
        settings.auto_ban_enabled = true;
        for (role, ids, ban) in [("top", vec![2, 1], 3), ("jungle", vec![4], 5)] {
            settings.profiles.insert(
                format!("classic:{role}"),
                AutomationProfile {
                    pick_ids: ids,
                    ban_id: Some(ban),
                    ..Default::default()
                },
            );
        }
        assert_eq!(
            AutoAcceptService::configured_action_pool(&settings, "classic", "top", false),
            vec![2, 1]
        );
        assert_eq!(
            AutoAcceptService::configured_action_pool(&settings, "classic", "jungle", false),
            vec![4]
        );
        assert_eq!(
            AutoAcceptService::configured_action_pool(&settings, "classic", "top", true),
            vec![3]
        );
        settings.profiles.get_mut("classic:top").unwrap().pick_ids = vec![6];
        settings.profiles.get_mut("classic:top").unwrap().ban_id = Some(7);
        assert_eq!(
            AutoAcceptService::configured_action_pool(&settings, "classic", "top", false),
            vec![6]
        );
        assert_eq!(
            AutoAcceptService::configured_action_pool(&settings, "classic", "top", true),
            vec![7]
        );
        settings.auto_pick_enabled = false;
        settings.auto_ban_enabled = false;
        assert!(
            AutoAcceptService::configured_action_pool(&settings, "classic", "top", false)
                .is_empty()
        );
        assert!(
            AutoAcceptService::configured_action_pool(&settings, "classic", "top", true).is_empty()
        );
        assert!(
            AutoAcceptService::configured_action_pool(&settings, "classic", "unknown", false)
                .is_empty()
        );
    }

    #[test]
    fn current_action_pool_respects_random_visible_count_and_arena_manual() {
        let mut settings = AutomationSettings::default();
        for mode in ["aram", "mayhem", "arena"] {
            settings.profiles.insert(
                mode.into(),
                AutomationProfile {
                    pick_ids: vec![3, 2, 1],
                    pick_count: Some(2),
                    auto_pick: true,
                    auto_ban: true,
                    ban_id: Some(4),
                    arena_pick: "manual".into(),
                    ..Default::default()
                },
            );
        }
        for mode in ["aram", "mayhem"] {
            assert_eq!(
                AutoAcceptService::configured_action_pool(&settings, mode, "unknown", false),
                vec![3, 2]
            );
            assert!(
                AutoAcceptService::configured_action_pool(&settings, mode, "unknown", true)
                    .is_empty()
            );
        }
        assert!(
            AutoAcceptService::configured_action_pool(&settings, "arena", "unknown", false)
                .is_empty()
        );
        assert_eq!(
            AutoAcceptService::configured_action_pool(&settings, "arena", "unknown", true),
            vec![4]
        );
    }

    #[tokio::test]
    #[ignore = "requires an explicit RUSTLM_TEST_LCU simulator config"]
    async fn simulator_consecutive_ready_checks_can_have_identical_timers() {
        assert!(std::env::var_os("RUSTLM_TEST_LCU").is_some());
        let client = Arc::new(RiotClientService::new());
        let service = AutoAcceptService::new(
            client.clone(),
            Arc::new(RunePagesStorage::for_test(
                std::env::temp_dir().join("rustlm-lcu-unused-runes.json"),
            )),
        );
        service.enabled.store(true, Ordering::SeqCst);
        for _ in 0..2 {
            client
                .lcu_post("/__test/scenario", r#"{"name":"ready"}"#)
                .await
                .unwrap();
            let ready: serde_json::Value = serde_json::from_str(
                &client
                    .lcu_get("/lol-matchmaking/v1/ready-check")
                    .await
                    .unwrap(),
            )
            .unwrap();
            let before: serde_json::Value =
                serde_json::from_str(&client.lcu_get("/__test/state").await.unwrap()).unwrap();
            service.handle_ready_check(&ready).await;
            service.handle_ready_check(&ready).await;
            let after: serde_json::Value =
                serde_json::from_str(&client.lcu_get("/__test/state").await.unwrap()).unwrap();
            assert_eq!(after["ready"]["playerResponse"], "Accepted");
            assert_eq!(
                after["actions"].as_array().unwrap().len(),
                before["actions"].as_array().unwrap().len() + 1
            );
        }
    }

    #[tokio::test]
    async fn changing_role_reapplies_loadout_without_repeating_the_same_profile() {
        let service = AutoAcceptService::new(
            Arc::new(RiotClientService::new()),
            Arc::new(RunePagesStorage::for_test(
                std::env::temp_dir().join("rustlm-lcu-unused-runes.json"),
            )),
        );
        service.sync_loadout_profile("classic", "middle").await;
        service.has_set_spells.store(true, Ordering::SeqCst);
        service.has_set_runes.store(true, Ordering::SeqCst);
        service.sync_loadout_profile("classic", "middle").await;
        assert!(service.has_set_spells.load(Ordering::SeqCst));
        assert!(service.has_set_runes.load(Ordering::SeqCst));
        *service.loadout_retry_at.lock().await =
            std::time::Instant::now() + Duration::from_secs(60);
        service.sync_loadout_profile("classic", "jungle").await;
        assert!(!service.has_set_spells.load(Ordering::SeqCst));
        assert!(!service.has_set_runes.load(Ordering::SeqCst));
        assert!(*service.loadout_retry_at.lock().await <= std::time::Instant::now());
    }
    #[tokio::test]
    async fn late_champion_resolution_cannot_overwrite_new_settings() {
        let service = AutoAcceptService::new(
            Arc::new(RiotClientService::new()),
            Arc::new(RunePagesStorage::for_test(
                std::env::temp_dir().join("rustlm-lcu-unused-runes.json"),
            )),
        );
        let mut saved = AutomationSettings::default();
        saved.auto_accept_method = "Polling".into();
        let revision = service.set_settings(saved.clone()).await;
        saved.pick_champion1_id = Some(1);
        service
            .refresh_settings_if_unchanged(saved.clone(), revision)
            .await;
        assert_eq!(service.get_settings().await.pick_champion1_id, Some(1));
        let revision = service.set_settings(saved.clone()).await;
        let mut edited = saved.clone();
        edited.auto_accept_method = "UIA".into();
        edited.pick_champion1_id = Some(2);
        service.set_settings(edited).await;
        service.refresh_settings_if_unchanged(saved, revision).await;
        let current = service.get_settings().await;
        assert_eq!(current.auto_accept_method, "UIA");
        assert_eq!(current.pick_champion1_id, Some(2));
        service.set_enabled_flag_only(false);
        assert!(!service.accept_match().await);
    }

    #[tokio::test]
    async fn automation_snapshot_keeps_current_accept_enabled_flag() {
        let service = AutoAcceptService::new(
            Arc::new(RiotClientService::new()),
            Arc::new(RunePagesStorage::for_test(
                std::env::temp_dir().join("rustlm-lcu-unused-runes.json"),
            )),
        );
        service.set_enabled_flag_only(true);
        assert!(service.get_settings().await.is_enabled);
        let settings = service.get_settings().await;
        service.set_enabled_flag_only(false);
        service.set_settings(settings).await;
        assert!(!service.get_settings().await.is_enabled);
        service.settings.write().await.auto_accept_method = "Polling".into();
        assert!(!service.accept_match().await);
    }

    #[tokio::test]
    #[ignore = "requires an explicit RUSTLM_TEST_LCU simulator config"]
    async fn simulator_ready_check_retries_failed_and_cancelled_attempts() {
        assert!(std::env::var_os("RUSTLM_TEST_LCU").is_some());
        let client = Arc::new(RiotClientService::new());
        let service = AutoAcceptService::new(
            client.clone(),
            Arc::new(RunePagesStorage::for_test(
                std::env::temp_dir().join("rustlm-lcu-unused-runes.json"),
            )),
        );
        service.enabled.store(true, Ordering::SeqCst);
        for cancelled in [false, true] {
            client
                .lcu_post("/__test/scenario", r#"{"name":"ready"}"#)
                .await
                .unwrap();
            let before: serde_json::Value =
                serde_json::from_str(&client.lcu_get("/__test/state").await.unwrap()).unwrap();
            let ready: serde_json::Value = serde_json::from_str(
                &client
                    .lcu_get("/lol-matchmaking/v1/ready-check")
                    .await
                    .unwrap(),
            )
            .unwrap();
            client.lcu_post("/__test/failure", &format!(r#"{{"route":"/lol-matchmaking/v1/ready-check/accept","status":503,"delayMs":{}}}"#, if cancelled {300} else {0})).await.unwrap();
            if cancelled {
                assert!(tokio::time::timeout(
                    Duration::from_millis(30),
                    service.handle_ready_check(&ready)
                )
                .await
                .is_err());
            } else {
                service.handle_ready_check(&ready).await;
            }
            assert!(!service.accept_in_progress.load(Ordering::SeqCst));
            service.handle_ready_check(&ready).await;
            let state: serde_json::Value =
                serde_json::from_str(&client.lcu_get("/__test/state").await.unwrap()).unwrap();
            assert_eq!(
                state["ready"]["playerResponse"], "Accepted",
                "same-timer retry must work after cancellation={cancelled}"
            );
            assert_eq!(
                state["actions"].as_array().unwrap().len(),
                before["actions"].as_array().unwrap().len() + 1
            );
        }
    }

    #[tokio::test]
    #[ignore = "requires an explicit RUSTLM_TEST_LCU simulator config"]
    async fn simulator_polling_and_websocket_accept_without_optional_status_api() {
        assert!(std::env::var_os("RUSTLM_TEST_LCU").is_some());
        let client = Arc::new(RiotClientService::new());
        client
            .lcu_post(
                "/__test/unavailable",
                r#"{"routes":["/lol-service-status/v1/shard-data"]}"#,
            )
            .await
            .unwrap();
        for method in ["Polling", "WebSocket", "WebSocketSafetyNet"] {
            client
                .lcu_post("/__test/scenario", r#"{"name":"ready"}"#)
                .await
                .unwrap();
            let service = Arc::new(AutoAcceptService::new(
                client.clone(),
                Arc::new(RunePagesStorage::for_test(
                    std::env::temp_dir().join("rustlm-lcu-unused-runes.json"),
                )),
            ));
            service.enabled.store(true, Ordering::SeqCst);
            service.settings.write().await.auto_accept_method = if method == "WebSocketSafetyNet" {
                "WebSocket"
            } else {
                method
            }
            .into();
            let (tx, rx) = tokio::sync::watch::channel(false);
            let task = {
                let service = service.clone();
                tokio::spawn(async move {
                    if method != "WebSocket" {
                        service.run_polling_listener(rx).await;
                    } else {
                        service.run_websocket_listener(rx).await;
                    }
                })
            };
            let accepted = tokio::time::timeout(Duration::from_secs(4), async {
                loop {
                    let state: serde_json::Value =
                        serde_json::from_str(&client.lcu_get("/__test/state").await.unwrap())
                            .unwrap();
                    if state["ready"]["playerResponse"] == "Accepted" {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(25)).await;
                }
            })
            .await
            .is_ok();
            tx.send(true).unwrap();
            tokio::time::timeout(Duration::from_secs(2), task)
                .await
                .unwrap()
                .unwrap();
            if !accepted {
                client
                    .lcu_post("/__test/unavailable", r#"{"routes":[]}"#)
                    .await
                    .unwrap();
                panic!(
                    "{method} did not accept: an optional status route must not gate ready-check"
                );
            }
        }
        client
            .lcu_post("/__test/unavailable", r#"{"routes":[]}"#)
            .await
            .unwrap();
    }

    // Run only against scripts/lcu-simulator.mjs, never against a real client.
    #[tokio::test]
    #[ignore = "requires an explicit RUSTLM_TEST_LCU simulator config"]
    async fn simulator_flow_uses_real_http_ws_reconnect_and_bench_guards() {
        assert!(std::env::var_os("RUSTLM_TEST_LCU").is_some());
        let client = Arc::new(RiotClientService::new());
        client
            .lcu_post("/__test/scenario", r#"{"name":"ready"}"#)
            .await
            .unwrap();
        let service = Arc::new(AutoAcceptService::new(
            client.clone(),
            Arc::new(RunePagesStorage::for_test(
                std::env::temp_dir().join("rustlm-lcu-unused-runes.json"),
            )),
        ));
        service.enabled.store(true, Ordering::SeqCst);
        service.settings.write().await.auto_accept_method = "WebSocket".into();
        let (tx, rx) = tokio::sync::watch::channel(false);
        let task = {
            let service = service.clone();
            tokio::spawn(async move {
                service.run_websocket_listener(rx).await;
            })
        };
        // Production starts both listeners: polling also wakes timed bench unlocks.
        let poll_task = {
            let service = service.clone();
            let rx = tx.subscribe();
            tokio::spawn(async move {
                service.run_polling_listener(rx).await;
            })
        };
        async fn wait_for(client: &RiotClientService, champion: Option<i64>) {
            tokio::time::timeout(Duration::from_secs(12), async {
                loop {
                    let state: serde_json::Value =
                        serde_json::from_str(&client.lcu_get("/__test/state").await.unwrap())
                            .unwrap();
                    let done = match champion {
                        Some(id) => {
                            state["session"]["myTeam"][0]["championId"].as_i64() == Some(id)
                        }
                        None => state["ready"]["playerResponse"] == "Accepted",
                    };
                    if done {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            })
            .await
            .expect("automation did not reach expected simulator state");
        }
        // No ready-check event was sent after subscribing: initial HTTP snapshot must accept it.
        wait_for(&client, None).await;
        // Cancelling an in-flight HTTP attempt must release the acceptance gate.
        client
            .lcu_post(
                "/__test/failure",
                r#"{"route":"/lol-matchmaking/v1/ready-check","status":503,"delayMs":300}"#,
            )
            .await
            .unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(30), service.accept_match())
                .await
                .is_err()
        );
        assert!(!service.accept_in_progress.load(Ordering::SeqCst));
        let mut settings = service.settings.read().await.clone();
        settings.profiles.insert(
            "aram".into(),
            AutomationProfile {
                auto_pick: true,
                pick_ids: vec![1, 2, 3],
                bench: true,
                ..Default::default()
            },
        );
        service.set_settings(settings).await;
        client
            .lcu_post("/__test/scenario", r#"{"name":"aram-locked"}"#)
            .await
            .unwrap();
        // Direct handler proves the locked pool cannot issue a swap.
        let body = client
            .lcu_get("/lol-champ-select/v1/session")
            .await
            .unwrap();
        service
            .handle_champ_select_session(&serde_json::from_str(&body).unwrap())
            .await;
        let state: serde_json::Value =
            serde_json::from_str(&client.lcu_get("/__test/state").await.unwrap()).unwrap();
        assert_eq!(state["session"]["myTeam"][0]["championId"], 3);
        // Reconnect must recover a session change that occurred with no socket subscribed.
        client.lcu_post("/__test/disconnect", "{}").await.unwrap();
        client
            .lcu_post("/__test/scenario", r#"{"name":"aram-open"}"#)
            .await
            .unwrap();
        wait_for(&client, Some(1)).await;
        for status in [401, 403, 404, 429, 500] {
            client
                .lcu_post(
                    "/__test/failure",
                    &format!(r#"{{"route":"/lol-gameflow/v1/session","status":{status}}}"#),
                )
                .await
                .unwrap();
            let error = client
                .lcu_get("/lol-gameflow/v1/session")
                .await
                .unwrap_err()
                .to_string();
            assert!(error.contains(&status.to_string()));
            assert!(!error.contains("synthetic private response"));
            assert!(client.lcu_get("/lol-gameflow/v1/session").await.is_ok());
        }
        for method in [
            reqwest::Method::POST,
            reqwest::Method::PUT,
            reqwest::Method::DELETE,
            reqwest::Method::PATCH,
        ] {
            let result = match method {
                reqwest::Method::POST => client.lcu_post("/unsupported", "{}").await,
                reqwest::Method::PUT => client.lcu_put("/unsupported", "{}").await,
                reqwest::Method::DELETE => client.lcu_delete("/unsupported").await,
                _ => client.lcu_patch("/unsupported", "{}").await,
            };
            assert!(result.unwrap_err().to_string().contains("404"));
        }
        // A new client can change both its port and token; never reuse the old connection.
        let connection = client.lcu_get("/__test/next-connection").await.unwrap();
        client.lcu_post("/__test/disconnect", "{}").await.unwrap();
        std::fs::write(std::env::var_os("RUSTLM_TEST_LCU").unwrap(), connection).unwrap();
        client
            .lcu_post("/__test/scenario", r#"{"name":"ready"}"#)
            .await
            .unwrap();
        wait_for(&client, None).await;
        tx.send(true).unwrap();
        tokio::time::timeout(Duration::from_secs(2), task)
            .await
            .unwrap()
            .unwrap();
        tokio::time::timeout(Duration::from_secs(2), poll_task)
            .await
            .unwrap()
            .unwrap();
    }
}

#[cfg(test)]
mod physical_preference_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn random_ban_requires_resolved_raw_intent_even_when_all_configured_bans_are_protected() {
        use crate::models::automation::AutomationProfile;
        let mut settings = AutomationSettings::default();
        settings.auto_ban_enabled = true;
        assert!(!AutoAcceptService::has_configured_ban_intent(
            &settings, "classic", "top"
        ));
        let mut profile = AutomationProfile::default();
        settings
            .profiles
            .insert("classic:top".into(), profile.clone());
        assert!(!AutoAcceptService::has_configured_ban_intent(
            &settings, "classic", "top"
        ));
        profile.bans = vec!["unresolved".into()];
        profile.ban_ids = vec![0, -1, 10001];
        profile.ban_id = Some(99);
        settings
            .profiles
            .insert("classic:top".into(), profile.clone());
        assert!(!AutoAcceptService::has_configured_ban_intent(
            &settings, "classic", "top"
        ));
        profile.ban_ids = vec![99];
        profile.pick_ids = vec![99];
        assert!(profile.active_ban_ids().is_empty());
        settings
            .profiles
            .insert("classic:top".into(), profile.clone());
        assert!(AutoAcceptService::has_configured_ban_intent(
            &settings, "classic", "top"
        ));
        assert!(!AutoAcceptService::has_configured_ban_intent(
            &settings, "classic", "jungle"
        ));
        profile.bans.clear();
        profile.ban_ids.clear();
        assert!(profile.active_ban_ids().is_empty());
        settings
            .profiles
            .insert("classic:top".into(), profile.clone());
        assert!(AutoAcceptService::has_configured_ban_intent(
            &settings, "classic", "top"
        ));
        profile.ban_id = Some(0);
        settings.profiles.insert("classic:top".into(), profile);
        assert!(!AutoAcceptService::has_configured_ban_intent(
            &settings, "classic", "top"
        ));
    }

    #[test]
    fn ban_pool_skips_protected_first_and_completed_ban_then_leaves_manual() {
        let mut session = serde_json::json!({"localPlayerCellId":0,
            "myTeam":[{"cellId":0},{"cellId":1,"championPickIntent":99}],
            "actions":[[{"type":"ban","completed":true,"championId":2}]]});
        let pool = [99, 1, 2];
        assert_eq!(
            AutoAcceptService::available_action_pool(&session, &pool, &pool, &[], true),
            vec![1]
        );
        session["myTeam"][1]["championId"] = serde_json::json!(1);
        assert!(
            AutoAcceptService::available_action_pool(&session, &pool, &pool, &[], true).is_empty()
        );
    }

    #[test]
    fn random_bans_use_only_actual_bannable_ids_and_exclude_all_selected_and_intents() {
        let session = json!({"localPlayerCellId":0,
            "myTeam":[{"cellId":0,"championPickIntent":1},{"cellId":1,"championPickIntent":2},{"cellId":2,"championId":3}],
            "theirTeam":[{"championId":4}],
            "bans":{"myTeamBans":[5],"theirTeamBans":[6]},
            "actions":[[{"type":"ban","completed":true,"championId":7},
                {"type":"pick","completed":true,"championId":8}]]});
        let actual = [1, 2, 3, 4, 5, 6, 7, 8, 9];
        assert_eq!(
            AutoAcceptService::available_action_pool(&session, &actual, &actual, &[], true),
            vec![9]
        );
        assert!(
            AutoAcceptService::available_action_pool(&session, &[9, 10], &[], &[], true).is_empty()
        );
        assert_eq!(
            AutoAcceptService::available_action_pool(&session, &[9, 10], &[9], &[], true),
            vec![9]
        );
    }

    #[test]
    fn action_availability_removes_banned_ally_and_outside_subset_champions() {
        let session = json!({"localPlayerCellId":0,"allowSubsetChampionPicks":true,"myTeam":[{"cellId":0,"championId":0},{"cellId":1,"championId":2}],"actions":[[{"type":"ban","completed":true,"championId":1}]]});
        assert_eq!(
            AutoAcceptService::available_action_pool(
                &session,
                &[1, 2, 3, 4],
                &[1, 2, 3, 4],
                &[1, 2, 3],
                false
            ),
            vec![3]
        );
    }

    #[test]
    fn random_availability_includes_current_and_only_eligible_bench_cards() {
        let session = json!({"localPlayerCellId":0,"myTeam":[{"cellId":0,"championId":1}],"benchEnabled":true,"benchChampions":[{"championId":2}],"timer":{"phase":"FINALIZATION","totalTimeInPhase":60000,"adjustedTimeLeftInPhase":55000}});
        assert_eq!(
            AutoAcceptService::available_bench_pool(&session, &[1, 2, 3], &[1, 2, 3], &[], true),
            vec![1, 2]
        );
        assert_eq!(
            AutoAcceptService::available_bench_pool(&session, &[1, 2, 3], &[1, 2, 3], &[], false),
            vec![1]
        );
    }
}

#[cfg(test)]
mod arena_pool_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn enemy_arena_picks_do_not_block_but_ally_picks_and_both_sides_bans_do() {
        let mut session = json!({"localPlayerCellId":1,"allowDuplicatePicks":true,
            "myTeam":[{"cellId":1,"subteamId":7,"championId":0},
                {"cellId":2,"subteamId":7,"championId":2},
                {"cellId":3,"subteamId":8,"championId":3}],
            "theirTeam":[{"championId":4}],"bans":{"myTeamBans":[5],"theirTeamBans":[6]},
            "actions":[[{"type":"ban","completed":true,"championId":7}]]});
        assert_eq!(
            AutoAcceptService::available_action_pool(
                &session,
                &[1, 2, 3, 4, 5, 6, 7],
                &[1, 2, 3, 4, 5, 6, 7],
                &[],
                false
            ),
            vec![1, 3, 4]
        );
        session["allowSubsetChampionPicks"] = json!(true);
        assert_eq!(
            AutoAcceptService::available_action_pool(
                &session,
                &[1, 2, 3, 4],
                &[1, 2, 3, 4],
                &[3],
                false
            ),
            vec![3]
        );
        assert!(AutoAcceptService::available_action_pool(
            &session,
            &[1, 2, 3, 4],
            &[1, 2, 3, 4],
            &[],
            false
        )
        .is_empty());
        assert!(
            AutoAcceptService::available_action_pool(&session, &[3], &[], &[3], false).is_empty()
        );
    }
}
