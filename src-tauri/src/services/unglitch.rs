use super::riot_client::RiotClientService;
use crate::models::unglitch::{UnglitchAction, UnglitchResult, UnglitchStatus};
use std::sync::Arc;
use std::time::Duration;

pub async fn status(client: &RiotClientService) -> Result<UnglitchStatus, String> {
    let (backend_running, ui_running, _) =
        tokio::task::spawn_blocking(RiotClientService::unglitch_process_state)
            .await
            .map_err(|_| "Не удалось проверить процессы League.")?;
    if ui_running || !backend_running {
        client.invalidate_cache();
    }
    let phase = if backend_running {
        tokio::time::timeout(
            Duration::from_secs(2),
            client.lcu_get("/lol-gameflow/v1/gameflow-phase"),
        )
        .await
        .ok()
        .and_then(Result::ok)
        .and_then(|text| serde_json::from_str::<String>(&text).ok())
        .filter(|phase| !phase.is_empty() && phase.len() <= 64)
    } else {
        None
    };
    Ok(UnglitchStatus {
        backend_running,
        ui_running,
        api_available: phase.is_some(),
        phase,
        force_supported: cfg!(windows),
    })
}

pub async fn execute(
    client: Arc<RiotClientService>,
    action: UnglitchAction,
    confirmed: bool,
) -> Result<UnglitchResult, String> {
    let before = status(&client).await?;
    if !before.backend_running {
        return Err("Клиент League не запущен. Откройте League через Riot Client.".into());
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
    for _ in 0..20 {
        tokio::time::sleep(Duration::from_millis(300)).await;
        let root = forced.as_ref().map(|(_, root)| root.clone());
        let (backend, ui, renderers) = tokio::task::spawn_blocking(move || {
            let (backend, ui, mut renderers) = RiotClientService::unglitch_process_state();
            if let Some(root) = root {
                renderers.retain(|pid| RiotClientService::unglitch_renderer_matches(*pid, &root));
            }
            (backend, ui, renderers)
        })
        .await
        .map_err(|_| "Не удалось проверить результат.")?;
        if !backend {
            return Err("Клиент League завершился. Откройте его через Riot Client.".into());
        }
        let ready = match action {
            UnglitchAction::Hide => !ui,
            UnglitchAction::ForceReload => {
                ui && forced
                    .as_ref()
                    .is_some_and(|(stopped, _)| renderers.iter().any(|pid| !stopped.contains(pid)))
            }
            _ => ui,
        };
        if ready {
            let after = status(&client).await?;
            let message = match action {
                UnglitchAction::Hide => "Интерфейс закрыт; клиент League продолжает работать.",
                UnglitchAction::Restore => "Интерфейс League открыт.",
                UnglitchAction::Reload => "Команда перезагрузки принята; интерфейс League запущен.",
                UnglitchAction::ForceReload => "Новый процесс рендера запущен.",
            };
            return Ok(UnglitchResult {
                message: message.into(),
                status: after,
            });
        }
    }
    Err("Команда отправлена, но изменение состояния пока не подтверждено. Обновите состояние перед повтором.".into())
}
