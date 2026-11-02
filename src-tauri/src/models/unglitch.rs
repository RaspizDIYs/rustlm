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
#[derive(Serialize)]
pub struct UnglitchStatus {
    pub backend_running: bool,
    pub ui_running: bool,
    pub api_available: bool,
    pub phase: Option<String>,
    pub force_supported: bool,
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
