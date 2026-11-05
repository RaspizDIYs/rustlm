use crate::services::lol_config::LolConfigService;
use std::{sync::Arc, time::Duration};

/// Journals outlive this task and the app; no process is ever terminated here.
pub async fn run(config: Arc<LolConfigService>) {
    loop {
        if let Err(error) = config.observe_shared_login().await {
            log::debug!("Guest settings recovery pending: {error}");
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}
