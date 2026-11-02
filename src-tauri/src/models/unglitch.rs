use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum UnglitchAction {
    Reload,
    ForceReload,
    Hide,
    Restore,
}
impl UnglitchAction {
    pub fn endpoint(self) -> Option<&'static str> {
        match self {
            Self::Reload => Some("/riotclient/kill-and-restart-ux"),
            Self::Hide => Some("/riotclient/kill-ux"),
            Self::Restore => Some("/riotclient/launch-ux"),
            Self::ForceReload => None,
        }
    }
    pub fn needs_confirmation(self, phase: Option<&str>) -> bool {
        match self {
            Self::ForceReload | Self::Hide => true,
            Self::Restore => false,
            Self::Reload => !matches!(phase, Some("None" | "Lobby" | "EndOfGame" | "PreEndOfGame")),
        }
    }
}
#[derive(Clone, Serialize)]
pub struct UnglitchStatus {
    pub backend_running: bool,
    pub ui_running: bool,
    pub api_available: bool,
    pub phase: Option<String>,
    pub force_supported: bool,
    pub window_responsive: Option<bool>,
    pub renderer_count: usize,
    pub response_ms: Option<u64>,
    pub game_running: bool,
    #[serde(skip)]
    pub backend_pid: Option<u32>,
    #[serde(skip)]
    pub ui_pid: Option<u32>,
    #[serde(skip)]
    pub scope_verified: bool,
}

#[derive(Default, Deserialize)]
pub struct UxProbe {
    pub backend_running: bool,
    pub ui_running: bool,
    pub backend_pid: Option<u32>,
    pub ui_pid: Option<u32>,
    pub renderer_count: usize,
    pub window_responsive: Option<bool>,
    pub game_running: bool,
    pub scope_verified: bool,
}

pub fn automatic_reload_allowed(status: &UnglitchStatus) -> bool {
    status.backend_running
        && status.backend_pid.is_some()
        && status.ui_pid.is_some()
        && status.ui_running
        && status.scope_verified
        && !status.game_running
        && status.api_available
        && matches!(
            status.phase.as_deref(),
            Some("None" | "Lobby" | "EndOfGame")
        )
}

#[derive(Clone, Serialize)]
pub struct WatchStatus {
    pub enabled: bool,
    pub state: String,
    pub last_result: Option<String>,
}

pub struct WatchPolicy {
    identity: Option<u32>,
    last_observation: Option<u64>,
    warm_until: u64,
    suspect_since: Option<u64>,
    healthy_since: Option<u64>,
    attempted: bool,
    last_attempt: Option<u64>,
    pause_until: u64,
}

impl Default for WatchPolicy {
    fn default() -> Self {
        Self {
            identity: None,
            last_observation: None,
            warm_until: 0,
            suspect_since: None,
            healthy_since: None,
            attempted: false,
            last_attempt: None,
            pause_until: 0,
        }
    }
}

impl WatchPolicy {
    pub fn pause(&mut self, now: u64) {
        self.pause_until = now.saturating_add(120);
        self.suspect_since = None;
    }

    pub fn observe(&mut self, now: u64, status: &UnglitchStatus) -> (&'static str, bool) {
        // A suspended PC/app has not supplied continuous evidence of a freeze.
        if self
            .last_observation
            .is_some_and(|last| now.saturating_sub(last) > 15)
        {
            self.suspect_since = None;
            self.healthy_since = None;
        }
        self.last_observation = Some(now);
        if self.identity != status.backend_pid {
            self.identity = status.backend_pid;
            self.warm_until = now.saturating_add(90);
            self.suspect_since = None;
            self.healthy_since = None;
            self.attempted = false;
        }
        let healthy = status.api_available
            && status.ui_running
            && status.window_responsive == Some(true)
            && status.renderer_count > 0;
        if healthy {
            let since = self.healthy_since.get_or_insert(now);
            if now.saturating_sub(*since) >= 60 {
                self.attempted = false;
            }
        } else {
            self.healthy_since = None;
        }
        let state = if !status.backend_running {
            "not_running"
        } else if !status.ui_running {
            "hidden"
        } else if now < self.pause_until {
            "paused"
        } else if !status.api_available {
            "no_connection"
        } else if !automatic_reload_allowed(status) {
            "protected"
        } else if now < self.warm_until {
            "starting"
        } else if status.window_responsive == Some(false)
            || (status.renderer_count == 0 && status.window_responsive.is_some())
        {
            "suspect"
        } else if healthy {
            "healthy"
        } else {
            "observing"
        };
        if state != "suspect" {
            self.suspect_since = None;
            return (state, false);
        }
        let since = self.suspect_since.get_or_insert(now);
        if self.attempted {
            return ("needs_attention", false);
        }
        if self
            .last_attempt
            .is_some_and(|last| now.saturating_sub(last) < 600)
        {
            return ("cooldown", false);
        }
        if now.saturating_sub(*since) < 30 {
            return ("suspect", false);
        }
        self.attempted = true;
        self.last_attempt = Some(now);
        ("recovering", true)
    }
}
#[derive(Serialize)]
pub struct UnglitchResult {
    pub message: String,
    pub status: UnglitchStatus,
}

pub fn renderer_in_install(executable: &Path, install: &Path) -> bool {
    executable.file_name().is_some_and(|name| {
        name.to_string_lossy()
            .eq_ignore_ascii_case("LeagueClientUxRender.exe")
    }) && executable.parent().is_some_and(|parent| {
        parent
            .to_string_lossy()
            .eq_ignore_ascii_case(&install.to_string_lossy())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sample() -> UnglitchStatus {
        UnglitchStatus {
            backend_running: true,
            ui_running: true,
            api_available: true,
            phase: Some("Lobby".into()),
            force_supported: true,
            window_responsive: Some(false),
            renderer_count: 1,
            response_ms: Some(5),
            game_running: false,
            backend_pid: Some(10),
            ui_pid: Some(11),
            scope_verified: true,
        }
    }

    #[test]
    fn watch_waits_for_startup_and_continuous_hang_then_attempts_only_once() {
        let mut policy = WatchPolicy::default();
        let status = sample();
        assert_eq!(policy.observe(0, &status), ("starting", false));
        assert_eq!(policy.observe(89, &status), ("starting", false));
        assert_eq!(policy.observe(90, &status), ("suspect", false));
        policy.observe(100, &status);
        policy.observe(110, &status);
        assert_eq!(policy.observe(119, &status), ("suspect", false));
        assert_eq!(policy.observe(120, &status), ("recovering", true));
        assert_eq!(policy.observe(3600, &status), ("needs_attention", false));
    }

    #[test]
    fn watch_never_reloads_critical_unknown_or_unverified_sessions() {
        for phase in [
            None,
            Some("Matchmaking"),
            Some("ReadyCheck"),
            Some("ChampSelect"),
            Some("GameStart"),
            Some("InProgress"),
            Some("Reconnect"),
            Some("PreEndOfGame"),
            Some("Unexpected"),
        ] {
            let mut status = sample();
            status.phase = phase.map(str::to_owned);
            let mut policy = WatchPolicy::default();
            for now in [0, 90, 120, 600] {
                assert!(!policy.observe(now, &status).1, "{phase:?}");
            }
        }
        for field in ["api", "scope", "game", "hidden"] {
            let mut status = sample();
            match field {
                "api" => status.api_available = false,
                "scope" => status.scope_verified = false,
                "game" => status.game_running = true,
                _ => status.ui_running = false,
            }
            let mut policy = WatchPolicy::default();
            for now in [0, 90, 120, 600] {
                assert!(!policy.observe(now, &status).1, "{field}");
            }
        }
    }

    #[test]
    fn slow_loads_and_intermittent_hangs_do_not_accumulate_into_a_restart() {
        let mut policy = WatchPolicy::default();
        let mut status = sample();
        status.window_responsive = Some(true);
        status.response_ms = Some(3500);
        for now in [0, 90, 120, 600] {
            assert!(!policy.observe(now, &status).1);
        }
        status.window_responsive = Some(false);
        assert!(!policy.observe(610, &status).1);
        status.window_responsive = Some(true);
        assert!(!policy.observe(625, &status).1);
        status.window_responsive = Some(false);
        assert!(!policy.observe(640, &status).1);
        policy.observe(650, &status);
        policy.observe(660, &status);
        assert!(!policy.observe(669, &status).1);
        assert!(policy.observe(670, &status).1);
    }

    #[test]
    fn recovery_requires_stable_health_and_cooldown_before_a_later_incident() {
        let mut policy = WatchPolicy::default();
        let mut status = sample();
        policy.observe(0, &status);
        policy.observe(90, &status);
        policy.observe(100, &status);
        policy.observe(110, &status);
        assert!(policy.observe(120, &status).1);
        status.window_responsive = Some(true);
        for now in (130..=190).step_by(10) {
            policy.observe(now, &status);
        }
        status.window_responsive = Some(false);
        assert_eq!(policy.observe(200, &status), ("cooldown", false));
        for now in (210..720).step_by(10) {
            assert!(!policy.observe(now, &status).1);
        }
        assert!(policy.observe(720, &status).1);
    }

    #[test]
    fn resuming_after_a_long_gap_requires_new_hang_evidence() {
        let mut policy = WatchPolicy::default();
        let status = sample();
        policy.observe(0, &status);
        policy.observe(90, &status);
        policy.observe(100, &status);
        assert_eq!(policy.observe(1000, &status), ("suspect", false));
        policy.observe(1010, &status);
        policy.observe(1020, &status);
        assert!(policy.observe(1030, &status).1);
    }

    #[test]
    fn manual_actions_and_new_backend_processes_restart_the_grace_period() {
        let mut policy = WatchPolicy::default();
        let mut status = sample();
        policy.observe(0, &status);
        policy.observe(90, &status);
        policy.pause(100);
        assert_eq!(policy.observe(120, &status), ("paused", false));
        assert_eq!(policy.observe(220, &status), ("suspect", false));
        status.backend_pid = Some(20);
        assert_eq!(policy.observe(230, &status), ("starting", false));
        assert_eq!(policy.observe(319, &status), ("starting", false));
        assert_eq!(policy.observe(320, &status), ("suspect", false));
    }

    #[test]
    fn only_the_three_fixed_lcu_endpoints_are_allowed() {
        assert_eq!(
            UnglitchAction::Reload.endpoint(),
            Some("/riotclient/kill-and-restart-ux")
        );
        assert_eq!(UnglitchAction::Hide.endpoint(), Some("/riotclient/kill-ux"));
        assert_eq!(
            UnglitchAction::Restore.endpoint(),
            Some("/riotclient/launch-ux")
        );
        assert_eq!(UnglitchAction::ForceReload.endpoint(), None);
        assert!(serde_json::from_str::<UnglitchAction>("\"kill_league\"").is_err());
    }
    #[test]
    fn disruptive_and_uncertain_actions_require_confirmation_but_restore_stays_available() {
        for phase in [None, Some("Lobby"), Some("ChampSelect"), Some("InProgress")] {
            assert!(UnglitchAction::ForceReload.needs_confirmation(phase));
            assert!(UnglitchAction::Hide.needs_confirmation(phase));
            assert!(!UnglitchAction::Restore.needs_confirmation(phase));
        }
        assert!(!UnglitchAction::Reload.needs_confirmation(Some("Lobby")));
        assert!(UnglitchAction::Reload.needs_confirmation(Some("ChampSelect")));
        assert!(UnglitchAction::Reload.needs_confirmation(None));
    }
    #[test]
    fn process_filter_cannot_target_the_backend_game_riot_or_another_install() {
        let root = std::env::temp_dir().join("riot-client-test");
        assert!(renderer_in_install(
            &root.join("LeagueClientUxRender.exe"),
            &root
        ));
        assert!(renderer_in_install(
            &root.join("leagueclientuxrender.EXE"),
            &root
        ));
        for name in [
            "LeagueClient.exe",
            "LeagueClientUx.exe",
            "League of Legends.exe",
            "RiotClientServices.exe",
            "vgc.exe",
            "LeagueClientUxRender.exe.bak",
        ] {
            assert!(!renderer_in_install(&root.join(name), &root));
        }
        assert!(!renderer_in_install(
            &root.join("other/LeagueClientUxRender.exe"),
            &root
        ));
    }
}
