use serde::Serialize;
use std::{
    collections::{BTreeMap, VecDeque},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Area {
    Accept,
    Champions,
    Matchmaking,
    Runes,
    Loot,
    Stats,
}

fn area(path: &str) -> Option<Area> {
    let path = path.split('?').next()?;
    if path.starts_with("/lol-matchmaking/v1/ready-check") {
        Some(Area::Accept)
    } else if path.starts_with("/lol-champ-select/") {
        Some(Area::Champions)
    } else if path.starts_with("/lol-matchmaking/") {
        Some(Area::Matchmaking)
    } else if path.starts_with("/lol-perks/") {
        Some(Area::Runes)
    } else if path.starts_with("/lol-loot/") {
        Some(Area::Loot)
    } else if ["/lol-match-history/", "/lol-ranked/", "/lol-summoner/"]
        .iter()
        .any(|p| path.starts_with(p))
    {
        Some(Area::Stats)
    } else {
        None
    }
}

pub fn selected_screen(name: &str) -> Option<Area> {
    match name.trim().to_lowercase().as_str() {
        "ремесло" | "loot" | "crafting" | "hextech crafting" => Some(Area::Loot),
        "статистика" | "statistics" | "stats" => Some(Area::Stats),
        _ => None,
    }
}

#[derive(Clone, Serialize)]
pub struct UiSample {
    pub state: &'static str,
    pub screen: Option<Area>,
    pub elements: usize,
    pub progress: usize,
    pub disabled_buttons: usize,
    pub partial: bool,
    pub waiting_ms: u64,
    #[serde(skip)]
    pub pid: Option<u32>,
}
impl UiSample {
    pub fn empty(state: &'static str) -> Self {
        Self {
            state,
            screen: None,
            elements: 0,
            progress: 0,
            disabled_buttons: 0,
            partial: false,
            waiting_ms: 0,
            pid: None,
        }
    }
}

#[derive(Clone, Serialize)]
pub struct RequestStats {
    pub area: Area,
    pub requests: u64,
    pub errors: u64,
    pub unavailable: u64,
    pub last_ms: u64,
    pub max_ms: u64,
}
#[derive(Clone, Serialize)]
pub struct DiagnosticEvent {
    pub id: u64,
    pub area: Area,
    pub outcome: &'static str,
    pub duration_ms: u64,
    pub status: Option<u16>,
}
#[derive(Clone, Serialize)]
pub struct Report {
    pub ui: UiSample,
    pub requests: Vec<RequestStats>,
    pub events: Vec<DiagnosticEvent>,
}

impl Report {
    pub fn export(&self, folder: &std::path::Path) -> Result<std::path::PathBuf, String> {
        std::fs::create_dir_all(folder).map_err(|_| "Не удалось открыть папку журналов.")?;
        let path = folder.join(format!(
            "client-diagnostics-{}-{:016x}.json",
            chrono::Utc::now().format("%Y%m%dT%H%M%S%3f"),
            rand::random::<u64>()
        ));
        let data = serde_json::to_vec_pretty(&serde_json::json!({
            "format": "rustlm-client-diagnostics", "version": 1,
            "app_version": env!("CARGO_PKG_VERSION"), "created_at": chrono::Utc::now().to_rfc3339(),
            "diagnostics": self,
        }))
        .map_err(|_| "Не удалось подготовить отчёт.")?;
        super::network::atomic_write(&path, &data).map_err(|_| "Не удалось сохранить отчёт.")?;
        Ok(path)
    }
}

#[derive(Default)]
struct Data {
    stats: BTreeMap<Area, RequestStats>,
    events: VecDeque<DiagnosticEvent>,
    next_id: u64,
    ui: Option<UiSample>,
    wait_since: Option<Instant>,
    last_ui_at: Option<Instant>,
}
#[derive(Default)]
pub struct ClientDiagnostics {
    data: Mutex<Data>,
    probing: AtomicBool,
}

impl ClientDiagnostics {
    fn push(
        data: &mut Data,
        area: Area,
        outcome: &'static str,
        duration_ms: u64,
        status: Option<u16>,
    ) {
        data.next_id += 1;
        let id = data.next_id;
        data.events.push_back(DiagnosticEvent {
            id,
            area,
            outcome,
            duration_ms,
            status,
        });
        if data.events.len() > 24 {
            data.events.pop_front();
        }
    }

    pub fn request(&self, method: &str, path: &str, status: Option<u16>, elapsed: Duration) {
        let Some(area) = area(path) else {
            return;
        };
        let ms = elapsed.as_millis().min(u64::MAX as u128) as u64;
        let unavailable = method == "GET" && status == Some(404);
        let failed = !unavailable && !status.is_some_and(|s| (200..300).contains(&s));
        let mut data = self.data.lock().unwrap();
        let stats = data.stats.entry(area).or_insert(RequestStats {
            area,
            requests: 0,
            errors: 0,
            unavailable: 0,
            last_ms: 0,
            max_ms: 0,
        });
        stats.requests += 1;
        stats.errors += u64::from(failed);
        stats.unavailable += u64::from(unavailable);
        stats.last_ms = ms;
        stats.max_ms = stats.max_ms.max(ms);
        // Do not fill history with normal polling or missing sessions outside champion select.
        if failed || (!unavailable && ms >= 1500) || method != "GET" {
            Self::push(
                &mut data,
                area,
                if failed {
                    "failed"
                } else if ms >= 1500 {
                    "slow_response"
                } else {
                    "response"
                },
                ms,
                status,
            );
        }
    }

    pub fn champion_result(&self, verified: bool, elapsed: Duration) {
        Self::push(
            &mut self.data.lock().unwrap(),
            Area::Champions,
            if verified {
                "lcu_confirmed"
            } else {
                "not_confirmed"
            },
            elapsed.as_millis() as u64,
            None,
        );
    }

    pub fn snapshot(&self) -> Report {
        let data = self.data.lock().unwrap();
        Report {
            ui: data
                .ui
                .clone()
                .unwrap_or_else(|| UiSample::empty("not_checked")),
            requests: data.stats.values().cloned().collect(),
            events: data.events.iter().rev().cloned().collect(),
        }
    }

    fn update_ui(&self, mut sample: UiSample) {
        let mut data = self.data.lock().unwrap();
        let same = data
            .last_ui_at
            .is_some_and(|time| time.elapsed() < Duration::from_secs(30))
            && data.ui.as_ref().is_some_and(|old| {
                old.pid == sample.pid && old.screen == sample.screen && old.progress > 0
            });
        data.last_ui_at = Some(Instant::now());
        if sample.progress > 0 && sample.state == "available" {
            if !same {
                data.wait_since = Some(Instant::now());
            }
            sample.waiting_ms = data
                .wait_since
                .map(|start| start.elapsed().as_millis() as u64)
                .unwrap_or(0);
        } else {
            data.wait_since = None;
        }
        data.ui = Some(sample);
    }

    pub async fn probe(self: &Arc<Self>) {
        #[cfg(debug_assertions)]
        if std::env::var_os("RUSTLM_TEST_LCU").is_some() {
            self.update_ui(UiSample::empty("simulated"));
            return;
        }
        if self
            .probing
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return;
        }
        let this = self.clone();
        let task = tokio::task::spawn_blocking(move || {
            struct Permit(Arc<ClientDiagnostics>);
            impl Drop for Permit {
                fn drop(&mut self) {
                    self.0.probing.store(false, Ordering::SeqCst);
                }
            }
            let _permit = Permit(this);
            let probe = super::riot_client::RiotClientService::unglitch_probe();
            if !probe.backend_running {
                return UiSample::empty("not_running");
            }
            if !probe.ui_running {
                return UiSample::empty("hidden");
            }
            if !probe.scope_verified || probe.window_responsive != Some(true) {
                return UiSample::empty("unavailable");
            }
            #[cfg(windows)]
            return super::uia_login::inspect_client_ui(probe.ui_pid.unwrap_or(0));
            #[cfg(not(windows))]
            UiSample::empty("unsupported")
        });
        let sample = match tokio::time::timeout(Duration::from_secs(3), task).await {
            Ok(Ok(sample)) => sample,
            _ => UiSample::empty("unavailable"),
        };
        // A timed-out provider retains the single permit until it actually returns; no thread accumulation.
        self.update_ui(sample);
    }

    pub async fn run(self: Arc<Self>, logger: Arc<super::file_logger::FileLogger>) {
        let mut last_id = 0;
        let mut last_state = String::new();
        loop {
            self.probe().await;
            let report = self.snapshot();
            let state = format!("{}:{}", report.ui.state, report.ui.waiting_ms >= 30000);
            if state != last_state {
                logger.info(&format!("[ClientDiagnostics] UIA: {state}"));
                last_state = state;
            }
            for event in report
                .events
                .iter()
                .rev()
                .filter(|event| event.id > last_id)
            {
                logger.info(&format!(
                    "[ClientDiagnostics] {:?}: {}; {} ms; HTTP {:?}",
                    event.area, event.outcome, event.duration_ms, event.status
                ));
            }
            if let Some(event) = report.events.first() {
                last_id = event.id;
            }
            tokio::time::sleep(Duration::from_secs(15)).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn export_is_versioned_unique_and_does_not_destroy_reports_when_folder_is_unavailable() {
        let folder = std::env::temp_dir().join(format!("rustlm-diag-{}", rand::random::<u128>()));
        let report = ClientDiagnostics::default().snapshot();
        let first = report.export(&folder).unwrap();
        let second = report.export(&folder).unwrap();
        assert_ne!(first, second);
        let data: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&first).unwrap()).unwrap();
        assert_eq!(data["format"], "rustlm-client-diagnostics");
        assert_eq!(data["version"], 1);
        assert_eq!(data["app_version"], env!("CARGO_PKG_VERSION"));
        assert!(chrono::DateTime::parse_from_rfc3339(data["created_at"].as_str().unwrap()).is_ok());
        assert_eq!(data["diagnostics"]["ui"]["state"], "not_checked");
        assert!(report.export(&first).unwrap_err().contains("папку"));
        assert!(first.is_file() && second.is_file());
        assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 2);
        std::fs::remove_dir_all(folder).unwrap();
    }
    #[tokio::test]
    #[ignore = "requires an explicit RUSTLM_TEST_LCU simulator config"]
    async fn simulator_transport_records_metrics_without_raw_payloads_or_real_ui() {
        assert!(std::env::var_os("RUSTLM_TEST_LCU").is_some());
        let client = super::super::riot_client::RiotClientService::new();
        client
            .lcu_post(
                "/__test/failure",
                r#"{"route":"/lol-loot/v1/player-loot","status":503}"#,
            )
            .await
            .unwrap();
        assert!(client.lcu_get("/lol-loot/v1/player-loot").await.is_err());
        assert!(client
            .lcu_get("/lol-ranked/v1/private-player")
            .await
            .is_err());
        client.diagnostics.probe().await;
        let report = client.diagnostics.snapshot();
        assert_eq!(report.ui.state, "simulated");
        assert_eq!(
            report
                .requests
                .iter()
                .find(|s| s.area == Area::Loot)
                .unwrap()
                .errors,
            1
        );
        assert_eq!(
            report
                .requests
                .iter()
                .find(|s| s.area == Area::Stats)
                .unwrap()
                .unavailable,
            1
        );
        let json = serde_json::to_string(&report).unwrap();
        assert!(!json.contains("private-player"));
        assert!(!json.contains("synthetic private response"));
    }

    #[test]
    fn waiting_indicators_are_not_hang_verdicts_and_gaps_reset_wait_time() {
        let diagnostics = ClientDiagnostics::default();
        let mut sample = UiSample::empty("available");
        sample.pid = Some(1);
        sample.progress = 1;
        diagnostics.update_ui(sample.clone());
        {
            let mut data = diagnostics.data.lock().unwrap();
            data.wait_since = Some(Instant::now() - Duration::from_secs(45));
        }
        diagnostics.update_ui(sample.clone());
        assert_eq!(diagnostics.snapshot().ui.state, "available");
        assert!(diagnostics.snapshot().ui.waiting_ms >= 45000);
        {
            let mut data = diagnostics.data.lock().unwrap();
            data.last_ui_at = Some(Instant::now() - Duration::from_secs(60));
        }
        diagnostics.update_ui(sample);
        assert!(diagnostics.snapshot().ui.waiting_ms < 1000);
    }

    #[test]
    fn missing_sessions_are_not_failures_and_fast_polling_does_not_flood_history() {
        let diagnostics = ClientDiagnostics::default();
        for _ in 0..50 {
            diagnostics.request(
                "GET",
                "/lol-champ-select/v1/session",
                Some(404),
                Duration::from_millis(5),
            );
        }
        let report = diagnostics.snapshot();
        assert!(report.events.is_empty());
        assert_eq!(report.requests[0].errors, 0);
        assert_eq!(report.requests[0].unavailable, 50);
    }
    #[test]
    fn diagnostics_are_bounded_and_never_store_paths_queries_bodies_or_account_names() {
        let diagnostics = ClientDiagnostics::default();
        for _ in 0..40 {
            diagnostics.request(
                "GET",
                "/lol-ranked/v1/private-account/secret?token=password",
                Some(500),
                Duration::from_secs(2),
            );
        }
        let report = diagnostics.snapshot();
        assert_eq!(report.events.len(), 24);
        let json = serde_json::to_string(&report).unwrap();
        for value in ["private-account", "secret", "token", "password"] {
            assert!(!json.contains(value));
        }
        assert_eq!(report.requests[0].errors, 40);
    }
    #[test]
    fn transport_acknowledgement_and_confirmed_champion_selection_are_distinct() {
        let diagnostics = ClientDiagnostics::default();
        diagnostics.request(
            "PATCH",
            "/lol-champ-select/v1/session/actions/7",
            Some(204),
            Duration::from_millis(50),
        );
        assert_eq!(diagnostics.snapshot().events[0].outcome, "response");
        diagnostics.champion_result(true, Duration::from_millis(800));
        assert_eq!(diagnostics.snapshot().events[0].outcome, "lcu_confirmed");
        diagnostics.champion_result(false, Duration::from_secs(2));
        assert_eq!(diagnostics.snapshot().events[0].outcome, "not_confirmed");
    }
    #[test]
    fn only_selected_known_tabs_identify_a_screen() {
        assert_eq!(selected_screen(" Ремесло "), Some(Area::Loot));
        assert_eq!(selected_screen("STATISTICS"), Some(Area::Stats));
        assert_eq!(selected_screen("Player#1234"), None);
    }
}
