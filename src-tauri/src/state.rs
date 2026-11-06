use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::sync::Mutex;

use crate::services::accounts_storage::AccountsStorage;
use crate::services::auto_accept::AutoAcceptService;
use crate::services::cloud_sync::CloudSyncService;
use crate::services::customization::CustomizationService;
use crate::services::data_dragon::DataDragonService;
use crate::services::file_logger::FileLogger;
use crate::services::goodluck_auth::GoodLuckAuthService;
use crate::services::lol_config::LolConfigService;
use crate::services::reveal::RevealService;
use crate::services::riot_client::RiotClientService;
use crate::services::rune_data::RuneDataService;
use crate::services::rune_pages_storage::RunePagesStorage;
use crate::services::settings::SettingsService;

pub struct AppState {
    pub lifecycle: crate::services::lifecycle::Lifecycle,
    pub logger: Arc<FileLogger>,
    pub settings: Arc<SettingsService>,
    pub accounts: Arc<AccountsStorage>,
    pub riot_client: Arc<RiotClientService>,
    pub data_dragon: Arc<DataDragonService>,
    pub rune_data: Arc<RuneDataService>,
    pub rune_pages: Arc<RunePagesStorage>,
    pub auto_accept: Arc<AutoAcceptService>,
    pub unglitch: Arc<crate::services::unglitch::UnglitchMonitor>,
    pub customization: Arc<CustomizationService>,
    pub reveal: Arc<RevealService>,
    pub goodluck: Arc<GoodLuckAuthService>,
    pub cloud_sync: Arc<CloudSyncService>,
    pub lol_config: Arc<LolConfigService>,
    pub login_cancelled: Arc<AtomicBool>,
    pub post_login_lcu_task: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl AppState {
    pub fn new() -> Self {
        let logger = Arc::new(FileLogger::new());
        let settings = Arc::new(SettingsService::new());
        let accounts = Arc::new(AccountsStorage::new());
        let riot_client = Arc::new(RiotClientService::new());
        let data_dragon = Arc::new(DataDragonService::new());
        let rune_data = Arc::new(RuneDataService::new());
        let rune_pages = Arc::new(RunePagesStorage::new());
        let auto_accept = Arc::new(AutoAcceptService::new(
            Arc::clone(&riot_client),
            Arc::clone(&rune_pages),
        ));
        let customization = Arc::new(CustomizationService::new(Arc::clone(&riot_client)));
        let unglitch = Arc::new(crate::services::unglitch::UnglitchMonitor::new(
            settings.load_setting("UnglitchAutoRecovery", true),
        ));
        let reveal = Arc::new(RevealService::new(
            Arc::clone(&riot_client),
            Arc::clone(&settings),
            Arc::clone(&logger),
        ));
        let goodluck = Arc::new(GoodLuckAuthService::new());
        let cloud_sync = Arc::new(CloudSyncService::new(
            Arc::clone(&accounts),
            Arc::clone(&settings),
            Arc::clone(&rune_pages),
            Arc::clone(&goodluck),
        ));
        let lol_config = Arc::new(LolConfigService::new(
            Arc::clone(&riot_client),
            Arc::clone(&logger),
        ));
        let login_cancelled = Arc::new(AtomicBool::new(false));

        logger.info("RustLM starting up...");

        Self {
            lifecycle: crate::services::lifecycle::Lifecycle::default(),
            logger,
            settings,
            accounts,
            riot_client,
            data_dragon,
            rune_data,
            rune_pages,
            auto_accept,
            unglitch,
            customization,
            reveal,
            goodluck,
            cloud_sync,
            lol_config,
            login_cancelled,
            post_login_lcu_task: Mutex::new(None),
        }
    }

    pub fn begin_shutdown(&self) -> bool {
        if !self.lifecycle.begin_close() {
            return false;
        }
        self.login_cancelled
            .store(true, std::sync::atomic::Ordering::SeqCst);
        self.unglitch.set_enabled(false);
        self.auto_accept.begin_shutdown();
        true
    }

    pub async fn shutdown(&self) -> (bool, bool) {
        let refresh = self.post_login_lcu_task.lock().unwrap().take();
        if let Some(task) = &refresh {
            task.abort();
        }
        let client = self.riot_client.clone();
        let logger = self.logger.clone();
        let recorder = tokio::task::spawn_blocking(move || {
            let saved = client.recorder.stop().is_ok();
            logger.info(if saved {
                "[Shutdown] Recorder stopped and synced; guest journal retained."
            } else {
                "[Shutdown] Recorder sync failed; guest journal retained."
            });
            saved
        });
        let listeners = self.auto_accept.shutdown();
        let (listeners, recorder) = tokio::join!(listeners, recorder);
        if let Some(task) = refresh {
            let _ = task.await;
        }
        (listeners, recorder.unwrap_or(false))
    }
}
