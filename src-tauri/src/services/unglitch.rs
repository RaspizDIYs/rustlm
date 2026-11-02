use super::riot_client::RiotClientService;
use crate::models::unglitch::{UnglitchAction, UnglitchResult, UnglitchStatus};
use crate::models::unglitch::{WatchPolicy, WatchStatus};
use std::sync::Arc;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex,
};
use std::time::Duration;

pub(crate) static ACTION: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

pub async fn status(client: &RiotClientService) -> Result<UnglitchStatus, String> {
    #[cfg(debug_assertions)]
    let simulated = std::env::var_os("RUSTLM_TEST_LCU").is_some();
    #[cfg(not(debug_assertions))]
    let simulated = false;
    let probe = if simulated {
        let body = client
            .lcu_get("/__test/ux-health")
            .await
            .map_err(|_| "Тестовый LCU недоступен.")?;
        serde_json::from_str::<crate::models::unglitch::UxProbe>(&body)
            .map_err(|_| "Некорректный тестовый снимок.")?
    } else {
        tokio::task::spawn_blocking(RiotClientService::unglitch_probe)
            .await
            .map_err(|_| "Не удалось проверить процессы League.")?
    };
    let mut phase = None;
    let mut api_available = false;
    let mut response_ms = None;
    if probe.backend_running {
        client.invalidate_cache();
        let started = std::time::Instant::now();
        if let Ok(Ok(text)) = tokio::time::timeout(
            Duration::from_secs(2),
            client.lcu_get("/lol-gameflow/v1/gameflow-phase"),
        )
        .await
        {
            api_available = true;
            phase = serde_json::from_str::<String>(&text)
                .ok()
                .filter(|phase| !phase.is_empty() && phase.len() <= 64);
        }
        if !api_available {
            api_available = matches!(
                tokio::time::timeout(
                    Duration::from_secs(2),
                    client.lcu_get("/lol-service-status/v1/shard-data")
                )
                .await,
                Ok(Ok(_))
            );
        }
        if api_available {
            response_ms = Some(started.elapsed().as_millis() as u64);
        }
    }
    Ok(UnglitchStatus {
        backend_running: probe.backend_running,
        ui_running: probe.ui_running,
        api_available,
        phase,
        force_supported: cfg!(windows) && !simulated,
        window_responsive: probe.window_responsive,
        renderer_count: probe.renderer_count,
        response_ms,
        game_running: probe.game_running,
        backend_pid: probe.backend_pid,
        ui_pid: probe.ui_pid,
        scope_verified: probe.scope_verified,
    })
}

pub async fn execute(
    client: Arc<RiotClientService>,
    action: UnglitchAction,
    confirmed: bool,
) -> Result<UnglitchResult, String> {
    run_action(client, action, confirmed, None).await
}

pub async fn execute_automatic(
    client: Arc<RiotClientService>,
    pid: u32,
    enabled: &std::sync::atomic::AtomicBool,
) -> Result<UnglitchResult, String> {
    run_action(client, UnglitchAction::Reload, false, Some((pid, enabled))).await
}

async fn run_action(
    client: Arc<RiotClientService>,
    action: UnglitchAction,
    confirmed: bool,
    automatic: Option<(u32, &std::sync::atomic::AtomicBool)>,
) -> Result<UnglitchResult, String> {
    let _guard = ACTION
        .try_lock()
        .map_err(|_| "Другая операция Unglitch ещё выполняется.")?;
    tokio::time::timeout(
        Duration::from_secs(25),
        execute_inner(client, action, confirmed, automatic),
    )
    .await
    .map_err(|_| {
        "Результат не подтверждён за 25 секунд. Повторный запуск автоматически не выполняется."
            .to_string()
    })?
}

async fn execute_inner(
    client: Arc<RiotClientService>,
    action: UnglitchAction,
    confirmed: bool,
    automatic: Option<(u32, &std::sync::atomic::AtomicBool)>,
) -> Result<UnglitchResult, String> {
    let before = status(&client).await?;
    if !before.backend_running {
        return Err("Клиент League не запущен. Откройте League через Riot Client.".into());
    }
    if let Some((pid, enabled)) = automatic {
        if !enabled.load(std::sync::atomic::Ordering::SeqCst)
            || before.backend_pid != Some(pid)
            || !crate::models::unglitch::automatic_reload_allowed(&before)
            || !(before.window_responsive == Some(false)
                || (before.renderer_count == 0 && before.window_responsive.is_some()))
        {
            return Err("Автовосстановление отменено: состояние клиента изменилось.".into());
        }
    }
    #[cfg(debug_assertions)]
    if action == UnglitchAction::ForceReload && std::env::var_os("RUSTLM_TEST_LCU").is_some() {
        return Err("Перезапуск реальных процессов отключён в тестовом LCU.".into());
    }
    if action.needs_confirmation(before.phase.as_deref()) && !confirmed {
        return Err("CONFIRM_REQUIRED".into());
    }
    if action == UnglitchAction::Hide && !before.api_available {
        return Err("Нет связи с клиентом League. Не удалось проверить возможность восстановления интерфейса.".into());
    }
    let forced = if action == UnglitchAction::ForceReload {
        Some(
            tokio::task::spawn_blocking(RiotClientService::restart_ux_renderers)
                .await
                .map_err(|_| "Не удалось проверить рендер клиента.")??,
        )
    } else {
        client
            .lcu_post(action.endpoint().ok_or("Неизвестное действие.")?, "")
            .await
            .map_err(|error| {
                let text = error.to_string();
                if text.contains("returned 404") {
                    "Эта функция недоступна в текущей версии клиента League.".to_string()
                } else if text.contains("returned 401") || text.contains("returned 403") {
                    "Клиент отклонил запрос. Обновите состояние и повторите попытку.".to_string()
                } else {
                    "Не удалось подтвердить выполнение команды. Обновите состояние перед повтором."
                        .to_string()
                }
            })?;
        None
    };
    let mut healthy_checks = 0;
    for _ in 0..20 {
        tokio::time::sleep(Duration::from_millis(300)).await;
        let after = status(&client).await?;
        if !after.backend_running {
            return Err("Клиент League завершился. Откройте его через Riot Client.".into());
        }
        let ready = match action {
            UnglitchAction::Hide => !after.ui_running,
            UnglitchAction::ForceReload => {
                let (stopped, root) = forced.as_ref().ok_or("Не удалось проверить рендер.")?;
                let root = root.clone();
                let current = tokio::task::spawn_blocking(move || {
                    let (_, _, renderers) = RiotClientService::unglitch_process_state();
                    renderers
                        .into_iter()
                        .filter(|pid| RiotClientService::unglitch_renderer_matches(*pid, &root))
                        .collect::<Vec<_>>()
                })
                .await
                .map_err(|_| "Не удалось проверить рендер.")?;
                after.ui_running && current.iter().any(|pid| !stopped.contains(pid))
            }
            UnglitchAction::Reload => {
                after.ui_running && after.ui_pid.is_some() && after.ui_pid != before.ui_pid
            }
            UnglitchAction::Restore => after.ui_running,
        };
        let healthy = after.api_available
            && after.window_responsive == Some(true)
            && after.renderer_count > 0;
        if ready && (action == UnglitchAction::Hide || healthy) {
            healthy_checks += 1;
        } else {
            healthy_checks = 0;
        }
        if healthy_checks >= if action == UnglitchAction::Hide { 1 } else { 3 } {
            let message = match action {
                UnglitchAction::Hide => "Интерфейс закрыт; клиент League продолжает работать.",
                UnglitchAction::Restore => "Интерфейс League открыт.",
                UnglitchAction::Reload => "Интерфейс перезагружен; окно и связь с League отвечают.",
                UnglitchAction::ForceReload => {
                    "Новый рендер запущен; окно и связь с League отвечают."
                }
            };
            return Ok(UnglitchResult {
                message: message.into(),
                status: after,
            });
        }
    }
    Err("Команда отправлена, но изменение состояния пока не подтверждено. Обновите состояние перед повтором.".into())
}

pub struct UnglitchMonitor {
    enabled: AtomicBool,
    started: std::time::Instant,
    policy: Mutex<WatchPolicy>,
    state: Mutex<WatchStatus>,
}

impl UnglitchMonitor {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled: AtomicBool::new(enabled),
            started: std::time::Instant::now(),
            policy: Mutex::new(WatchPolicy::default()),
            state: Mutex::new(WatchStatus {
                enabled,
                state: if enabled { "starting" } else { "disabled" }.into(),
                last_result: None,
            }),
        }
    }

    pub fn snapshot(&self) -> WatchStatus {
        self.state.lock().unwrap().clone()
    }

    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::SeqCst);
        self.pause_for_manual_action();
        let mut state = self.state.lock().unwrap();
        state.enabled = enabled;
        state.state = if enabled { "paused" } else { "disabled" }.into();
    }

    pub fn pause_for_manual_action(&self) {
        self.policy
            .lock()
            .unwrap()
            .pause(self.started.elapsed().as_secs());
    }

    async fn tick(&self, client: Arc<RiotClientService>, now: u64) -> Option<String> {
        if !self.enabled.load(Ordering::SeqCst) {
            return None;
        }
        let sample = match status(&client).await {
            Ok(sample) => sample,
            Err(_) => {
                self.policy.lock().unwrap().pause(now);
                let mut snapshot = self.state.lock().unwrap();
                if snapshot.enabled {
                    snapshot.state = "no_connection".into();
                }
                return None;
            }
        };
        if !sample.api_available {
            client.invalidate_cache();
        }
        let (state, recover) = self.policy.lock().unwrap().observe(now, &sample);
        if !self.enabled.load(Ordering::SeqCst) {
            return None;
        }
        {
            let mut snapshot = self.state.lock().unwrap();
            if !snapshot.enabled {
                return None;
            }
            snapshot.state = state.into();
        }
        if !recover {
            return None;
        }
        let result =
            execute_automatic(client, sample.backend_pid.unwrap_or(0), &self.enabled).await;
        let recovered = result.is_ok();
        let message = match result {
            Ok(result) => result.message,
            Err(error) => error,
        };
        let mut snapshot = self.state.lock().unwrap();
        snapshot.last_result = Some(message.clone());
        snapshot.state = if !self.enabled.load(Ordering::SeqCst) {
            "disabled"
        } else if recovered {
            "recovered"
        } else {
            "needs_attention"
        }
        .into();
        Some(message)
    }

    pub async fn run(
        self: Arc<Self>,
        client: Arc<RiotClientService>,
        logger: Arc<super::file_logger::FileLogger>,
    ) {
        let mut last_state = String::new();
        loop {
            if let Some(result) = self
                .tick(client.clone(), self.started.elapsed().as_secs())
                .await
            {
                logger.info(&format!("[Unglitch] Automatic recovery: {result}"));
            }
            let state = self.snapshot().state;
            if state != last_state {
                logger.info(&format!("[Unglitch] Watch: {state}"));
                last_state = state;
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    }
}

#[cfg(all(test, debug_assertions))]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "requires an explicit RUSTLM_TEST_LCU simulator config"]
    async fn simulator_watch_recovers_once_and_does_not_loop_after_failure() {
        assert!(std::env::var_os("RUSTLM_TEST_LCU").is_some());
        let client = Arc::new(RiotClientService::new());
        client
            .lcu_post("/__test/scenario", r#"{"name":"idle"}"#)
            .await
            .unwrap();
        client.lcu_post("/__test/ux-health", r#"{"window_responsive":false,"renderer_count":1,"game_running":false,"reloadSucceeds":true}"#).await.unwrap();
        client
            .lcu_post(
                "/__test/failure",
                r#"{"route":"/lol-gameflow/v1/gameflow-phase","status":404}"#,
            )
            .await
            .unwrap();
        let partial = status(&client).await.unwrap();
        assert!(partial.api_available);
        assert!(partial.phase.is_none());
        assert!(!crate::models::unglitch::automatic_reload_allowed(&partial));
        let monitor = UnglitchMonitor::new(true);
        assert!(monitor.tick(client.clone(), 0).await.is_none());
        assert!(monitor.tick(client.clone(), 90).await.is_none());
        monitor.tick(client.clone(), 100).await;
        monitor.tick(client.clone(), 110).await;
        assert!(monitor.tick(client.clone(), 119).await.is_none());
        assert!(monitor.tick(client.clone(), 120).await.is_some());
        assert_eq!(monitor.snapshot().state, "recovered");
        let healthy = status(&client).await.unwrap();
        assert_eq!(healthy.window_responsive, Some(true));
        assert!(healthy.api_available);
        assert!(healthy.renderer_count > 0);

        // Startup has passed, but a protected phase still cannot be reloaded.
        client
            .lcu_post("/__test/scenario", r#"{"name":"classic"}"#)
            .await
            .unwrap();
        client
            .lcu_post("/__test/ux-health", r#"{"window_responsive":false}"#)
            .await
            .unwrap();
        let protected = UnglitchMonitor::new(true);
        for now in [0, 90, 120, 600] {
            assert!(protected.tick(client.clone(), now).await.is_none());
        }
        assert_eq!(protected.snapshot().state, "protected");
        let enabled = AtomicBool::new(true);
        assert!(execute_automatic(client.clone(), 100, &enabled)
            .await
            .err()
            .unwrap()
            .contains("отменено"));

        client
            .lcu_post("/__test/scenario", r#"{"name":"idle"}"#)
            .await
            .unwrap();
        client
            .lcu_post(
                "/__test/ux-health",
                r#"{"window_responsive":false,"reloadSucceeds":false}"#,
            )
            .await
            .unwrap();
        let failing = UnglitchMonitor::new(true);
        failing.tick(client.clone(), 0).await;
        failing.tick(client.clone(), 90).await;
        failing.tick(client.clone(), 100).await;
        failing.tick(client.clone(), 110).await;
        assert!(failing.tick(client.clone(), 120).await.is_some());
        assert_eq!(failing.snapshot().state, "needs_attention");
        assert!(failing.tick(client.clone(), 1000).await.is_none());
        let body = client.lcu_get("/__test/state").await.unwrap();
        let state: serde_json::Value = serde_json::from_str(&body).unwrap();
        let reloads = state["actions"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|action| action["route"] == "/riotclient/kill-and-restart-ux")
            .count();
        assert_eq!(reloads, 2);
        failing.set_enabled(false);
        assert!(failing.tick(client.clone(), 2000).await.is_none());
        assert_eq!(failing.snapshot().state, "disabled");
        assert!(execute_automatic(client.clone(), 100, &failing.enabled)
            .await
            .err()
            .unwrap()
            .contains("отменено"));

        // An existing operation must exclude both manual and automatic commands.
        let _guard = ACTION.lock().await;
        assert!(execute(client, UnglitchAction::Reload, true)
            .await
            .err()
            .unwrap()
            .contains("Другая операция"));
    }
}
