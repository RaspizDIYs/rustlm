use crate::{
    error::AppError,
    services::{
        crypto::{dpapi_protect, dpapi_unprotect},
        riot_friends,
    },
    state::AppState,
};
use serde_json::{json, Value};
use tauri::State;

#[cfg(feature = "goodluck-test")]
const BASE: &str = "https://test.gltournament.ru/api/rustlm/league";
#[cfg(not(feature = "goodluck-test"))]
const BASE: &str = "https://gltournament.ru/api/rustlm/league";
pub(super) static LEAGUE_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

pub(super) async fn request(
    method: reqwest::Method,
    path: &str,
    token: Option<&str>,
    body: Option<Value>,
) -> Result<Value, AppError> {
    static HTTP: tokio::sync::OnceCell<reqwest::Client> = tokio::sync::OnceCell::const_new();
    let client = HTTP
        .get_or_try_init(|| async {
            reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(25))
                .redirect(reqwest::redirect::Policy::none())
                .build()
        })
        .await?;
    let mut req = client.request(method, format!("{BASE}{path}"));
    if let Some(token) = token {
        req = req.header("Authorization", format!("LM {token}"));
    }
    if let Some(body) = body {
        req = req.json(&body);
    }
    let response = req
        .send()
        .await
        .map_err(|error| {
            let code = if error.is_timeout() { "LM_NETWORK_TIMEOUT" } else { "LM_NETWORK_UNAVAILABLE" };
            log::warn!("[LeagueSharing] {} {code}", if path.starts_with("/party") { "party" } else { "league" });
            AppError::Custom(format!("{code}: не удалось связаться с сервером LM. Проверьте подключение"))
        })?;
    let status = response.status();
    if !status.is_success() {
        log::warn!("[LeagueSharing] {} HTTP_{}", if path.starts_with("/party") { "party" } else { "league" }, status.as_u16());
        // Never surface the response body: sharing endpoints handle credentials.
        let message = match status.as_u16() {
            401 => "Подключите текущий аккаунт League к LM заново",
            503 if path.starts_with("/party") => "Пати LM пока не включена на сервере",
            403 if path.starts_with("/party") => {
                "Это действие недоступно для данного участника группы"
            }
            404 if path.starts_with("/party") => {
                "Группа закрыта или приглашение больше не действует"
            }
            409 if path.starts_with("/party") => {
                "Группа изменилась. Обновите состав, готовность или договорённость"
            }
            429 if path.starts_with("/party") => {
                "Подождите немного перед следующим действием с группой"
            }
            403 => "Друг недоступен для передачи: обновите списки на обоих ПК",
            404 => "Предложение недоступно или срок доступа истёк",
            409 if path == "/offers" => {
                "Этот аккаунт уже предложен другу. Обновите список передач или отзовите прежнюю"
            }
            409 => "Riot ещё не обновил значок или предложение уже изменилось",
            429 if path == "/offers" => {
                "Слишком много передач. Закройте старые предложения или повторите позже"
            }
            429 if path == "/presence" => "Список обновляется слишком часто. Подождите минуту",
            429 => "Слишком много попыток. Повторите подключение через 10 минут",
            503 => "Независимые друзья League пока не включены на сервере",
            _ => "Сервер друзей League не завершил операцию",
        };
        let retry = response.headers().get(reqwest::header::RETRY_AFTER).and_then(|value| value.to_str().ok()).and_then(|value| value.parse::<u64>().ok()).map(|value| value.clamp(1,600));
        return Err(AppError::Custom(format!("{message}{} (HTTP {})", retry.map(|seconds| format!("; Retry-After: {seconds}")).unwrap_or_default(), status.as_u16())));
    }
    if status == reqwest::StatusCode::NO_CONTENT {
        return Ok(Value::Null);
    }
    response
        .json()
        .await
        .map_err(|_| AppError::Custom("Не удалось прочитать ответ сервера друзей League".into()))
}
fn bind_identity(
    mut me: riot_friends::CurrentRiotAccount,
    known: &str,
) -> riot_friends::CurrentRiotAccount {
    if !crate::models::account::is_valid_puuid(&me.puuid) {
        me.puuid = if crate::models::account::is_valid_puuid(known) {
            known.into()
        } else {
            String::new()
        };
    }
    me
}
pub(super) async fn identity(
    state: &AppState,
) -> Result<riot_friends::CurrentRiotAccount, AppError> {
    let me = riot_friends::current_account(&state.riot_client).await?;
    if !me
        .riot_id
        .rsplit_once('#')
        .map(|(name, tag)| !name.is_empty() && !tag.is_empty())
        .unwrap_or(false)
    {
        return Err(AppError::Custom(
            "Дождитесь загрузки аккаунта League".into(),
        ));
    }
    let known: String = state.settings.load_setting(
        &format!("LeaguePuuid:{}", me.riot_id.to_lowercase()),
        String::new(),
    );
    Ok(bind_identity(me, &known))
}
fn connection(state: &AppState) -> Option<Value> {
    let raw: String = state
        .settings
        .load_setting("LeagueConnection", String::new());
    let value: Value = serde_json::from_str(&dpapi_unprotect(&raw).ok()?).ok()?;
    valid_connection(&value).then_some(value)
}
fn valid_connection(value: &Value) -> bool {
    value["expires"]
        .as_i64()
        .map(|v| v > chrono::Utc::now().timestamp())
        .unwrap_or(false)
        && value["token"]
            .as_str()
            .map(|v| v.len() == 64 && v.bytes().all(|c| c.is_ascii_hexdigit()))
            .unwrap_or(false)
        && value["puuid"]
            .as_str()
            .map(crate::models::account::is_valid_puuid)
            .unwrap_or(false)
}

pub(super) async fn session(state: &AppState) -> Result<String, AppError> {
    connection(state)
        .and_then(|v| v["token"].as_str().map(str::to_owned))
        .ok_or_else(|| AppError::Custom("Подключите аккаунт League к LM".into()))
}
fn valid_id(id: &str) -> Result<(), AppError> {
    if id.len() == 32 && id.bytes().all(|c| c.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(AppError::Custom(
            "Неверный идентификатор предложения".into(),
        ))
    }
}

#[tauri::command]
pub async fn league_status(state: State<'_, AppState>) -> Result<Value, AppError> {
    let current = identity(&state).await.ok();
    let active = connection(&state);
    let mut recovery_error = None;
    // Status polling must not hold party authentication for a minute or hammer icon limits.
    static LAST_RECOVERY: std::sync::Mutex<Option<std::time::Instant>> =
        std::sync::Mutex::new(None);
    if let (Some(me), Ok(_guard)) = (current.as_ref(), LEAGUE_LOCK.try_lock()) {
        let key = format!("LeagueRestoreIcon:{}", me.puuid);
        if state
            .settings
            .load_setting::<Option<u64>>(&key, None)
            .is_some()
        {
            let attempt = LAST_RECOVERY
                .lock()
                .map(|mut last| {
                    if last.is_some_and(|at| at.elapsed() < std::time::Duration::from_secs(30)) {
                        false
                    } else {
                        *last = Some(std::time::Instant::now());
                        true
                    }
                })
                .unwrap_or(false);
            if attempt {
                recovery_error = match tokio::time::timeout(
                    std::time::Duration::from_secs(2),
                    restore_icon(&state, me),
                )
                .await
                {
                    Ok(Ok(())) => None,
                    Ok(Err(error)) => Some(error),
                    Err(_) => Some(AppError::Custom(
                        "Возврат исходного значка отложен — повторите позже".into(),
                    )),
                };
            }
        }
    }
    if active.is_none() {
        if let Some(error) = recovery_error {
            return Err(error);
        }
    }
    let pending = current.as_ref().is_some_and(|me| {
        state
            .settings
            .load_setting::<Option<u64>>(&format!("LeagueRestoreIcon:{}", me.puuid), None)
            .is_some()
    });
    Ok(
        json!({"connected":active.is_some(),"riot_id":current.as_ref().map(|v|&v.riot_id),
        "puuid":current.as_ref().map(|v|&v.puuid), "owner_riot_id":active.as_ref().and_then(|v|v["riot_id"].as_str()),
        "warning":if pending {Some("Исходный значок будет восстановлен позже")} else {None},
        "matches_current":active.as_ref().and_then(|v|v["puuid"].as_str()) == current.as_ref().map(|v|v.puuid.as_str()) && active.is_some()}),
    )
}

#[tauri::command]
pub async fn league_connect(state: State<'_, AppState>) -> Result<(), AppError> {
    let _guard = LEAGUE_LOCK
        .try_lock()
        .map_err(|_| AppError::Custom("Подключение уже выполняется".into()))?;
    let mut me = riot_friends::current_account(&state.riot_client).await?;
    let phase = state
        .riot_client
        .lcu_get("/lol-gameflow/v1/gameflow-phase")
        .await?;
    if !matches!(phase.trim_matches('"'), "None" | "Lobby") {
        return Err(AppError::Custom(
            "Подключите LM вне поиска, выбора чемпионов и матча".into(),
        ));
    }
    if let Some(active) = connection(&state) {
        let current = identity(&state).await?;
        if active["puuid"].as_str() == Some(current.puuid.as_str())
            && active["riot_id"].as_str().map(str::to_lowercase)
                == Some(current.riot_id.to_lowercase())
        {
            return restore_icon(&state, &current).await;
        }
    }
    let summoner: Value = serde_json::from_str(
        &state
            .riot_client
            .lcu_get("/lol-summoner/v1/current-summoner")
            .await?,
    )?;
    let (name, tag) = me
        .riot_id
        .rsplit_once('#')
        .filter(|(name, tag)| !name.is_empty() && !tag.is_empty())
        .ok_or_else(|| AppError::Custom("Дождитесь загрузки Riot ID".into()))?;
    let pending_key = format!("LeaguePendingProof:{}", me.riot_id.to_lowercase());
    let raw: String = state.settings.load_setting(&pending_key, String::new());
    let pending = if raw.is_empty() {
        Value::Null
    } else {
        serde_json::from_str::<Value>(&dpapi_unprotect(&raw)?)?
    };
    let now = chrono::Utc::now().timestamp();
    let reusable = reusable_proof(&pending, &me.riot_id, now);
    let deadline = if reusable {
        pending["deadline"].as_i64().unwrap()
    } else {
        now + 600
    };
    let mut proof = if reusable {
        pending["proof"].clone()
    } else {
        request(
            reqwest::Method::POST,
            "/connect",
            None,
            Some(json!({"gameName":name,"tagLine":tag})),
        )
        .await?
    };
    let canonical = proof["puuid"]
        .as_str()
        .filter(|p| crate::models::account::is_valid_puuid(p))
        .ok_or_else(|| AppError::Custom("Сервер не вернул настоящий идентификатор Riot".into()))?
        .to_owned();
    if proof["riot_id"].as_str().map(|v| v.to_lowercase()) != Some(me.riot_id.to_lowercase())
        || (crate::models::account::is_valid_puuid(&me.puuid) && me.puuid != canonical)
        || identity(&state).await?.riot_id.to_lowercase() != me.riot_id.to_lowercase()
    {
        return Err(AppError::Custom(
            "Аккаунт League изменился во время подключения".into(),
        ));
    }
    me.puuid = canonical;
    let restore_key = format!("LeagueRestoreIcon:{}", me.puuid);
    let previous: Option<u64> = state.settings.load_setting(&restore_key, None);
    let original = previous
        .or_else(|| summoner["profileIconId"].as_u64())
        .ok_or_else(|| AppError::Custom("Не удалось сохранить исходный значок League".into()))?;
    state.settings.save_settings(&[
        (&restore_key, json!(original)),
        (
            &format!("LeaguePuuid:{}", me.riot_id.to_lowercase()),
            json!(me.puuid),
        ),
    ])?;
    let secret = if reusable {
        pending["secret"].as_str()
    } else {
        proof["proof"].as_str()
    }
    .filter(|s| s.len() == 64 && s.bytes().all(|c| c.is_ascii_hexdigit()))
    .ok_or_else(|| AppError::Custom("Не получено подтверждение сервера".into()))?
    .to_owned();
    save_proof(&state, &pending_key, &me.riot_id, deadline, &secret, &proof)?;
    let result: Result<(), AppError> = async {
        for stage in 0..=3 {
            if proof["complete"] == true {
                let token = proof["token"].as_str().filter(|s| s.len() == 64 && s.bytes().all(|c| c.is_ascii_hexdigit()))
                    .ok_or_else(|| AppError::Custom("Не получен ключ LM".into()))?;
                if proof["identity"]["Puuid"].as_str().or_else(|| proof["identity"]["puuid"].as_str()) != Some(me.puuid.as_str()) {
                    return Err(AppError::Custom("Сервер подтвердил другой аккаунт".into()));
                }
                let protected = dpapi_protect(json!({"token":token,"puuid":me.puuid,"riot_id":me.riot_id,"expires":chrono::Utc::now().timestamp()+30*86400}).to_string().as_bytes())?;
                state.settings.save_settings(&[("LeagueConnection",json!(protected)),("LeagueFriendsVisible",json!(true)),(&pending_key, json!(""))])?;
                return Ok(());
            }
            if stage == 3 {
                break;
            }
            let remaining = deadline - chrono::Utc::now().timestamp();
            if remaining <= 0 {
                return Err(AppError::Custom("Срок подтверждения League истёк".into()));
            }
            let icon = proof["icon"].as_u64().filter(|v| (1..=28).contains(v))
                .ok_or_else(|| AppError::Custom("Неверный значок подтверждения".into()))?;
            let step = proof["step"].as_u64().filter(|v| *v <= 3)
                .ok_or_else(|| AppError::Custom("Неверный этап подтверждения".into()))?;
            let next = tokio::time::timeout(std::time::Duration::from_secs(remaining.min(180) as u64), async {
                set_icon(&state, &me, icon).await?;
                let mut attempt = 0;
                let mut delay = 3;
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
                    ensure_icon_account(&state, &me).await?;
                    match request(reqwest::Method::POST, "/connect/confirm", None, Some(json!({"proof":secret,"step":step}))).await {
                        Ok(value) => return Ok::<Value, AppError>(value),
                        Err(error) if retryable_confirm(&error) => {
                            delay = confirmation_delay(&error, attempt);
                            attempt += 1;
                        }
                        Err(error) => return Err(error),
                    }
                }
            }).await.map_err(|_| { log::warn!("[LeagueSharing] RIOT_PROOF_PENDING"); AppError::Custom("RIOT_PROOF_PENDING: подтверждение Riot пока не завершено. Этап сохранён; повторите подключение для продолжения".into()) })??;
            // Confirm responses omit the initial binding; keep it with every persisted step.
            if !next.is_object() {
                return Err(AppError::Custom("Сервер вернул неверное подтверждение League".into()));
            }
            proof = next;
            proof["puuid"] = json!(me.puuid);
            proof["riot_id"] = json!(me.riot_id);
            save_proof(&state, &pending_key, &me.riot_id, deadline, &secret, &proof)?;
        }
        Err(AppError::Custom("Подтверждение League не завершено. Повторите для продолжения".into()))
    }.await;
    let restored = restore_icon(&state, &me).await;
    match (result, restored) {
        (Err(error), Err(restore)) => Err(AppError::Custom(format!(
            "{error}. Восстановление значка: {restore}"
        ))),
        (Err(error), _) => Err(error),
        (Ok(()), Err(error)) => {
            log::warn!("League connected; original icon recovery remains pending: {error}");
            Ok(())
        }
        (Ok(()), Ok(())) => Ok(()),
    }
}

fn reusable_proof(pending: &Value, riot_id: &str, now: i64) -> bool {
    pending["riot_id"].as_str().map(str::to_lowercase) == Some(riot_id.to_lowercase())
        && pending["deadline"]
            .as_i64()
            .map(|d| d > now && d <= now + 600)
            .unwrap_or(false)
        && pending["proof"].is_object()
        && pending["secret"]
            .as_str()
            .map(|s| s.len() == 64 && s.bytes().all(|c| c.is_ascii_hexdigit()))
            .unwrap_or(false)
}

fn save_proof(
    state: &AppState,
    key: &str,
    riot_id: &str,
    deadline: i64,
    secret: &str,
    proof: &Value,
) -> Result<(), AppError> {
    let protected = dpapi_protect(
        json!({"riot_id":riot_id,"deadline":deadline,"secret":secret,"proof":proof})
            .to_string()
            .as_bytes(),
    )?;
    state.settings.save_setting(key, &json!(protected))
}

fn is_status(error: &AppError, status: u16) -> bool {
    match error {
        AppError::Http(error) => error.status().map(|s| s.as_u16()) == Some(status),
        AppError::Custom(message) | AppError::LcuNotDispatched(message) => {
            message.ends_with(&format!("(HTTP {status})"))
                || (message.starts_with("LCU PUT /lol-summoner/v1/current-summoner/icon returned ")
                    && message
                        .split(" returned ")
                        .nth(1)
                        .and_then(|s| s.split_whitespace().next())
                        == Some(status.to_string().as_str()))
        }
        _ => false,
    }
}

fn retryable_confirm(error: &AppError) -> bool {
    is_status(error, 409) || is_status(error, 429)
}

fn confirmation_delay(error: &AppError, attempt: u32) -> u64 {
    if is_status(error, 429) {
        if let AppError::Custom(message) | AppError::LcuNotDispatched(message) = error {
            if let Some(milliseconds) = message.split("retry after ").nth(1).and_then(|value| value.split_whitespace().next()).and_then(|value| value.parse::<u64>().ok()) { return milliseconds.div_ceil(1000).clamp(1,600); }
            if let Some(seconds) = message.split("Retry-After: ").nth(1).and_then(|value| value.split_whitespace().next()).and_then(|value| value.parse::<u64>().ok()) {
                return seconds.clamp(1,600);
            }
        }
        return retry_delay(attempt);
    }
    retry_delay(attempt).min(12)
}

fn retry_delay(attempt: u32) -> u64 {
    3 * (1_u64 << attempt.min(3))
}

async fn ensure_icon_account(
    state: &AppState,
    expected: &riot_friends::CurrentRiotAccount,
) -> Result<(), AppError> {
    let current = identity(state).await?;
    if current.puuid != expected.puuid
        || current.riot_id.to_lowercase() != expected.riot_id.to_lowercase()
    {
        return Err(AppError::Custom(
            "Аккаунт League изменился: значок не изменён".into(),
        ));
    }
    let phase = state
        .riot_client
        .lcu_get("/lol-gameflow/v1/gameflow-phase")
        .await?;
    if !matches!(phase.trim_matches('"'), "None" | "Lobby") {
        return Err(AppError::Custom(
            "Изменение значка отложено до выхода из поиска, выбора чемпионов и матча".into(),
        ));
    }
    Ok(())
}

async fn set_icon(
    state: &AppState,
    me: &riot_friends::CurrentRiotAccount,
    icon: u64,
) -> Result<(), AppError> {
    for attempt in 0..4 {
        ensure_icon_account(state, me).await?;
        match state
            .riot_client
            .lcu_put_checked(
                "/lol-summoner/v1/current-summoner/icon",
                &json!({"profileIconId":icon}).to_string(),
                || async { ensure_icon_account(state, me).await.is_ok() },
            )
            .await
        {
            Ok(_) => return Ok(()),
            Err(error) if is_status(&error, 429) && attempt < 3 => {
                tokio::time::sleep(std::time::Duration::from_secs(confirmation_delay(&error, attempt))).await;
            }
            Err(error) => return Err(error),
        }
    }
    unreachable!()
}

async fn restore_icon(
    state: &AppState,
    me: &riot_friends::CurrentRiotAccount,
) -> Result<(), AppError> {
    let key = format!("LeagueRestoreIcon:{}", me.puuid);
    let pending: Option<u64> = state.settings.load_setting(&key, None);
    if let Some(icon) = pending {
        tokio::time::timeout(
            std::time::Duration::from_secs(60),
            set_icon(state, me, icon),
        )
        .await
        .map_err(|_| {
            AppError::Custom("Восстановление значка отложено. Обновите статус League позже".into())
        })??;
        state.settings.save_setting(&key, &Value::Null)?;
    }
    Ok(())
}

#[tauri::command]
pub async fn league_refresh(visible: bool, state: State<'_, AppState>) -> Result<Value, AppError> {
    let _guard = LEAGUE_LOCK.lock().await;
    let active = connection(&state)
        .ok_or_else(|| AppError::Custom("Подключите аккаунт League к LM".into()))?;
    let secret = active["token"]
        .as_str()
        .ok_or_else(|| AppError::Custom("Подключите LM заново".into()))?
        .to_owned();
    let me = identity(&state).await.ok();
    let matches = me
        .as_ref()
        .map(|v| Some(v.puuid.as_str()) == active["puuid"].as_str())
        .unwrap_or(false);
    // Persist the opt-out intent even if the remote update times out; the next refresh retries it.
    state
        .settings
        .save_setting("LeagueFriendsVisible", &json!(visible))?;
    if !visible {
        request(
            reqwest::Method::PUT,
            "/presence",
            Some(&secret),
            Some(json!({"friends":[],"visible":false})),
        )
        .await?;
    } else if matches {
        let friends: Value =
            serde_json::from_str(&state.riot_client.lcu_get("/lol-chat/v1/friends").await?)?;
        let roster: Vec<&str> = friends
            .as_array()
            .ok_or_else(|| AppError::Custom("Список друзей League ещё не загружен".into()))?
            .iter()
            .filter_map(|f| f["puuid"].as_str())
            .filter(|p| crate::models::account::is_valid_puuid(p))
            .collect();
        if identity(&state).await?.puuid != me.unwrap().puuid {
            return Err(AppError::Custom("Аккаунт League изменился".into()));
        }
        request(
            reqwest::Method::PUT,
            "/presence",
            Some(&secret),
            Some(json!({"friends":roster,"visible":true})),
        )
        .await?;
    }
    let friends = request(reqwest::Method::GET, "/friends", Some(&secret), None).await?;
    let offers = request(reqwest::Method::GET, "/offers", Some(&secret), None).await?;
    if !friends.is_array() || !offers.is_array() {
        return Err(AppError::Custom(
            "Сервер вернул несовместимый список друзей или передач".into(),
        ));
    }
    Ok(json!({"friends":friends,"offers":offers}))
}

#[tauri::command]
pub async fn league_share(
    recipient: String,
    username: String,
    hours: i32,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    if !(1..=720).contains(&hours) || !crate::models::account::is_valid_puuid(&recipient) {
        return Err(AppError::Custom("Выберите друга и срок до 30 дней".into()));
    }
    let _guard = LEAGUE_LOCK.lock().await;
    let secret = session(&state).await?;
    let account = state
        .accounts
        .load_all()
        .into_iter()
        .find(|a| a.username == username)
        .ok_or_else(|| AppError::Custom("Аккаунт не найден".into()))?;
    let password = state.accounts.unprotect(&account.encrypted_password)?;
    request(reqwest::Method::POST,"/offers",Some(&secret),Some(json!({"recipient":recipient,"username":account.username,"password":password,"riotId":account.riot_id,"server":account.server,"hours":hours}))).await?;
    Ok(())
}
#[tauri::command]
pub async fn league_decide(
    id: String,
    action: String,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    let _guard = LEAGUE_LOCK.lock().await;
    valid_id(&id)?;
    if !matches!(action.as_str(), "accept" | "decline" | "revoke") {
        return Err(AppError::Custom("Неверное действие".into()));
    }
    request(
        reqwest::Method::POST,
        &format!("/offers/{id}"),
        Some(&session(&state).await?),
        Some(json!({"action":action})),
    )
    .await?;
    Ok(())
}
#[tauri::command]
pub async fn league_remove_friend(
    puuid: String,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    let _guard = LEAGUE_LOCK.lock().await;
    if !crate::models::account::is_valid_puuid(&puuid) {
        return Err(AppError::Custom("Неверный друг League".into()));
    }
    request(
        reqwest::Method::DELETE,
        &format!("/friends/{puuid}"),
        Some(&session(&state).await?),
        None,
    )
    .await?;
    Ok(())
}
pub async fn credentials(id: &str, state: &AppState) -> Result<(String, String), AppError> {
    let _guard = LEAGUE_LOCK.lock().await;
    valid_id(id)?;
    let value = request(
        reqwest::Method::POST,
        &format!("/offers/{id}/credentials"),
        Some(&session(state).await?),
        None,
    )
    .await?;
    let username = value["username"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| AppError::Custom("Не получен логин".into()))?;
    let password = value["password"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| AppError::Custom("Не получен пароль".into()))?;
    Ok((username.into(), password.into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resume_is_bound_to_account_and_original_finite_deadline() {
        let pending = json!({"riot_id":"Player#RU1", "deadline":1600,
            "secret":"a".repeat(64), "proof":{"step":2,"icon":17}});
        assert!(reusable_proof(&pending, "player#ru1", 1000));
        assert!(reusable_proof(&pending, "Player#RU1", 1599));
        assert!(!reusable_proof(&pending, "Other#RU1", 1000));
        assert!(!reusable_proof(&pending, "Player#RU1", 1600));
        assert!(!reusable_proof(&pending, "Player#RU1", 999));
        for (field, value) in [
            ("secret", json!("invalid")),
            ("proof", Value::Null),
            ("deadline", json!("later")),
        ] {
            let mut corrupted = pending.clone();
            corrupted[field] = value;
            assert!(!reusable_proof(&corrupted, "Player#RU1", 1000));
        }
    }

    #[test]
    fn confirmation_wait_respects_retry_after_and_limits_propagation_polling() {
        for (message, expected) in [("wait; Retry-After: 120 (HTTP 429)",120),("wait; Retry-After: 0 (HTTP 429)",1),("wait; Retry-After: 99999 (HTTP 429)",600)] {
            assert_eq!(confirmation_delay(&AppError::Custom(message.into()),0),expected);
        }
        let cooldown=AppError::LcuNotDispatched("LCU PUT /lol-summoner/v1/current-summoner/icon returned 429 Too Many Requests; retry after 59999 ms".into());
        assert!(is_status(&cooldown,429));assert_eq!(confirmation_delay(&cooldown,0),60);
        assert_eq!(confirmation_delay(&AppError::Custom("wait (HTTP 409)".into()),99),12);
        assert_eq!(confirmation_delay(&AppError::Custom("wait; Retry-After: invalid (HTTP 429)".into()),99),24);
    }

    #[test]
    fn only_propagation_and_rate_limits_are_retried_with_capped_backoff() {
        for status in [409, 429] {
            assert!(retryable_confirm(&AppError::Custom(format!(
                "message (HTTP {status})"
            ))));
        }
        for status in [400, 401, 403, 404, 500, 503] {
            assert!(!retryable_confirm(&AppError::Custom(format!(
                "message (HTTP {status})"
            ))));
        }
        assert!(is_status(
            &AppError::Custom(
                "LCU PUT /lol-summoner/v1/current-summoner/icon returned 429 Too Many Requests"
                    .into()
            ),
            429
        ));
        assert!(!is_status(
            &AppError::Custom("unrelated error mentioning HTTP 429".into()),
            429
        ));
        assert_eq!(
            (0..5).map(retry_delay).collect::<Vec<_>>(),
            vec![3, 6, 12, 24, 24]
        );
    }

    #[test]
    fn offer_identifiers_cannot_escape_the_fixed_api_route() {
        assert!(valid_id(&"a".repeat(32)).is_ok());
        for value in [
            "",
            "../connect",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/",
            "éééééééééééééééé",
        ] {
            assert!(valid_id(value).is_err());
        }
    }
    #[test]
    fn expired_or_corrupted_connection_is_not_reported_as_connected() {
        let valid = json!({"expires":chrono::Utc::now().timestamp()+60,"token":"a".repeat(64),"puuid":"b".repeat(78)});
        assert!(valid_connection(&valid));
        for (field, value) in [
            ("expires", json!(0)),
            ("expires", json!("later")),
            ("token", json!("")),
            ("token", json!("g".repeat(64))),
            ("puuid", json!("legacy-guid")),
        ] {
            let mut invalid = valid.clone();
            invalid[field] = value;
            assert!(!valid_connection(&invalid));
        }
        assert!(!valid_connection(&Value::Null));
    }
    #[test]
    fn guid_is_only_a_local_placeholder_and_cannot_override_canonical_identity() {
        let raw = riot_friends::CurrentRiotAccount {
            puuid: "legacy-guid".into(),
            riot_id: "Player#RU1".into(),
        };
        assert!(bind_identity(raw.clone(), "").puuid.is_empty());
        assert_eq!(bind_identity(raw, &"a".repeat(78)).puuid, "a".repeat(78));
        let canonical = riot_friends::CurrentRiotAccount {
            puuid: "b".repeat(78),
            riot_id: "Player#RU1".into(),
        };
        assert_eq!(
            bind_identity(canonical, &"a".repeat(78)).puuid,
            "b".repeat(78)
        );
    }
}
