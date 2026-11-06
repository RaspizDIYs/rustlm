pub mod accounts_storage;
pub(crate) type ClientAction = std::sync::Arc<ClientActionGuard>;

pub(crate) struct ClientActionGuard {
    _guard: tokio::sync::OwnedMutexGuard<()>,
    config_login: std::sync::OnceLock<std::sync::Arc<std::sync::atomic::AtomicBool>>,
}

impl ClientActionGuard {
    pub(crate) fn new(guard: tokio::sync::OwnedMutexGuard<()>) -> Self {
        Self {
            _guard: guard,
            config_login: std::sync::OnceLock::new(),
        }
    }

    pub(crate) fn retain_config_login(&self, flag: std::sync::Arc<std::sync::atomic::AtomicBool>) {
        self.config_login.get_or_init(|| flag);
    }
}

impl Drop for ClientActionGuard {
    fn drop(&mut self) {
        // Workers share this owner; recovery resumes only after the last one exits.
        if let Some(flag) = self.config_login.get() {
            flag.store(false, std::sync::atomic::Ordering::SeqCst);
        }
    }
}
pub(crate) fn client_action() -> &'static std::sync::Arc<tokio::sync::Mutex<()>> {
    static ACTION: std::sync::OnceLock<std::sync::Arc<tokio::sync::Mutex<()>>> = std::sync::OnceLock::new();
    ACTION.get_or_init(|| std::sync::Arc::new(tokio::sync::Mutex::new(())))
}
pub mod auto_accept;
pub mod lifecycle;
pub mod cloud_sync;
pub mod crypto;
pub mod customization;
pub mod data_dragon;
pub mod file_logger;
pub mod goodluck_auth;
pub mod lol_config;
pub mod reveal;
pub mod riot_client;
mod lcu_request_policy;
pub mod rune_data;
pub mod rune_pages_storage;
pub mod settings;
pub mod network;
pub mod network_hosts;
pub mod repair;
pub mod repair_paths;
#[cfg(windows)]
pub mod uia_login;

pub mod vault;

pub mod webview_repair;

pub mod champ_select_policy;

pub mod unglitch;
pub mod lcu_protocol;
pub mod client_diagnostics;

pub mod riot_friends;

pub mod draft_coordination;

pub mod guest_config_watcher;

pub mod session_recording;

pub mod riot_game;
