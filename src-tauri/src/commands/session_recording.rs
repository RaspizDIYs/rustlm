use crate::{services::session_recording::RecordingStatus, state::AppState};
use tauri::State;

#[tauri::command]
pub fn session_recording_status(state: State<'_, AppState>) -> RecordingStatus {
    state.riot_client.recorder.status()
}
#[tauri::command]
pub async fn session_recording_start(
    state: State<'_, AppState>,
) -> Result<RecordingStatus, String> {
    let folder = dirs::data_dir()
        .ok_or("Папка приложения недоступна")?
        .join("com.rustlm.app")
        .join("sessions");
    state.riot_client.recorder.start(folder)?;
    let _ = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        tokio::join!(
            state.riot_client.lcu_get("/lol-gameflow/v1/session"),
            state.riot_client.lcu_get("/lol-matchmaking/v1/ready-check"),
            state.riot_client.lcu_get("/lol-champ-select/v1/session"),
        )
    })
    .await;
    Ok(state.riot_client.recorder.status())
}
#[tauri::command]
pub fn session_recording_stop(state: State<'_, AppState>) -> Result<RecordingStatus, String> {
    state.riot_client.recorder.stop()
}
