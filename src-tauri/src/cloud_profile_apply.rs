use crate::error::AppError;
use crate::models::account::CloudProfilePayload;
use crate::state::AppState;

pub async fn apply_cloud_profile(
    state: &AppState,
    payload: &CloudProfilePayload,
) -> Result<usize, AppError> {
    let imported = state.accounts.import_from_cloud(payload.accounts.clone())?;
    if let Some(ref m) = payload.settings {
        state.settings.replace_settings_json_map(m.clone())?;
    }
    if let Some(ref pages) = payload.rune_pages {
        state.rune_pages.save_all(pages)?;
    }
    if let Some(ref us) = payload.update_settings {
        state.settings.save_update_settings(us)?;
    }
    crate::commands::auto_accept::load_persisted_automation_settings(state);
    state.reveal.reload_from_settings().await;
    Ok(imported)
}
