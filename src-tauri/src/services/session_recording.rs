//! Opt-in, bounded LCU traces. Authentication, chat and arbitrary endpoints are never recorded.
use serde::Serialize;
use serde_json::Value;
use std::{
    fs::{File, OpenOptions},
    io::Write,
    path::PathBuf,
    sync::Mutex,
    time::Instant,
};

#[derive(Default)]
pub struct SessionRecorder(Mutex<Option<Recording>>, Mutex<Option<RecordingStatus>>);
struct Recording {
    file: File,
    path: PathBuf,
    start: Instant,
    bytes: usize,
    count: usize,
}
#[derive(Clone, Serialize)]
pub struct RecordingStatus {
    pub active: bool,
    pub path: Option<String>,
    pub entries: usize,
    pub stopped_reason: Option<&'static str>,
}

fn storage_available(
    folder: &std::path::Path,
    max_bytes: u64,
    max_files: usize,
) -> Result<bool, String> {
    let mut bytes = 0u64;
    let mut files = 0usize;
    for entry in std::fs::read_dir(folder).map_err(|_| "Не удалось проверить папку записей")?
    {
        let entry = entry.map_err(|_| "Не удалось прочитать папку записей")?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with("lcu-") || !name.ends_with(".jsonl") {
            continue;
        }
        if !entry
            .file_type()
            .map_err(|_| "Не удалось проверить файл записи")?
            .is_file()
        {
            continue;
        }
        bytes = bytes.saturating_add(
            entry
                .metadata()
                .map_err(|_| "Не удалось проверить размер записи")?
                .len(),
        );
        files += 1;
        if bytes > max_bytes || files >= max_files {
            return Ok(false);
        }
    }
    Ok(true)
}

pub fn allowed_route(route: &str) -> bool {
    matches!(
        route,
        "/lol-gameflow/v1/gameflow-phase"
            | "/lol-gameflow/v1/session"
            | "/lol-matchmaking/v1/ready-check"
            | "/lol-matchmaking/v1/ready-check/accept"
            | "/lol-champ-select/v1/session"
            | "/lol-champ-select/v1/pickable-champion-ids"
            | "/lol-champ-select/v1/bannable-champion-ids"
            | "/lol-champ-select/v1/session/my-selection"
            | "/lol-lobby-team-builder/champ-select/v1/subset-champion-list"
            | "/lol-lobby-team-builder/champ-select/v1/crowd-favorte-champion-list"
            | "/lol-gameflow/v1/session/per-position-summoner-spells/required"
            | "/lol-gameflow/v1/session/per-position-summoner-spells/disallowed"
            | "/lol-perks/v1/pages"
    ) || [
        "/lol-champ-select/v1/session/actions/",
        "/lol-champ-select/v1/session/bench/swap/",
        "/lol-perks/v1/pages/",
    ]
    .iter()
    .any(|prefix| {
        route
            .strip_prefix(prefix)
            .is_some_and(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()))
    })
}

// Whitelist, rather than a blacklist: new Riot fields cannot silently introduce personal data.
pub fn sanitize(value: &Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .filter(|(key, _)| {
                    matches!(
                        key.as_str(),
                        "phase"
                            | "gameData"
                            | "queue"
                            | "id"
                            | "queueId"
                            | "queueType"
                            | "gameMode"
                            | "mapId"
                            | "gameId"
                            | "state"
                            | "playerResponse"
                            | "timer"
                            | "localPlayerCellId"
                            | "myTeam"
                            | "theirTeam"
                            | "cellId"
                            | "championId"
                            | "assignedPosition"
                            | "actions"
                            | "actorCellId"
                            | "type"
                            | "completed"
                            | "isInProgress"
                            | "isAllyAction"
                            | "pickTurn"
                            | "benchEnabled"
                            | "benchChampions"
                            | "allowSubsetChampionPicks"
                            | "championPickIntent"
                            | "bans"
                            | "myTeamBans"
                            | "theirTeamBans"
                            | "numBans"
                            | "skipChampionSelect"
                            | "team"
                            | "subteamId"
                            | "numPlayersPerTeam"
                            | "spell1Id"
                            | "spell2Id"
                            | "spells"
                            | "top"
                            | "jungle"
                            | "middle"
                            | "bottom"
                            | "utility"
                            | "*"
                            | "UNSELECTED"
                            | "name"
                            | "primaryStyleId"
                            | "subStyleId"
                            | "selectedPerkIds"
                            | "current"
                            | "adjustedTimeLeftInPhase"
                            | "totalTimeInPhase"
                            | "internalNowInEpochMs"
                            | "isCustomGame"
                            | "isSpectating"
                            | "allowRerolling"
                            | "rerollsRemaining"
                            | "championIds"
                    )
                })
                .map(|(key, value)| (key.clone(), sanitize(value)))
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().take(1000).map(sanitize).collect()),
        Value::String(s) => {
            // Only protocol enum values survive, never arbitrary names or server error messages.
            if matches!(
                s.as_str(),
                "None"
                    | "Lobby"
                    | "Matchmaking"
                    | "ReadyCheck"
                    | "ChampSelect"
                    | "GameStart"
                    | "InProgress"
                    | "WaitingForStats"
                    | "PreEndOfGame"
                    | "EndOfGame"
                    | "Reconnect"
                    | "TerminatedInError"
                    | "Invalid"
                    | "Accepted"
                    | "Declined"
                    | "Done"
                    | "BAN_PICK"
                    | "FINALIZATION"
                    | "PLANNING"
                    | "GAME_STARTING"
                    | "pick"
                    | "ban"
                    | "CLASSIC"
                    | "ARAM"
                    | "ARAM_MAYHEM"
                    | "KIWI"
                    | "CHERRY"
                    | "RustLM"
                    | "top"
                    | "jungle"
                    | "middle"
                    | "bottom"
                    | "utility"
                    | ""
            ) {
                value.clone()
            } else {
                Value::Null
            }
        }
        _ => value.clone(),
    }
}

impl SessionRecorder {
    pub fn active(&self) -> bool {
        let Ok(mut state) = self.0.lock() else {
            return false;
        };
        self.check_limit(&mut state);
        state.is_some()
    }
    pub fn start(&self, folder: PathBuf) -> Result<RecordingStatus, String> {
        let mut state = self.0.lock().map_err(|_| "Запись недоступна")?;
        if state.is_some() {
            return Err("Запись уже идёт".into());
        }
        std::fs::create_dir_all(&folder).map_err(|_| "Не удалось создать папку записи")?;
        // Reserve one bounded session; never delete the user's diagnostic evidence automatically.
        if !storage_available(&folder, 479 * 1024 * 1024, 1000)? {
            return Err(
                "Папка записей заполнена (500 МБ или 1000 файлов). Перенесите старые записи."
                    .into(),
            );
        }
        let path = folder.join(format!("lcu-{}.jsonl", rand::random::<u128>()));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|_| "Не удалось создать запись")?;
        if file
            .write_all(b"{\"version\":1,\"format\":\"rustlm-lcu-session\"}\n")
            .is_err()
        {
            drop(file);
            let _ = std::fs::remove_file(&path);
            return Err("Не удалось записать файл".into());
        }
        *state = Some(Recording {
            file,
            path,
            start: Instant::now(),
            bytes: 0,
            count: 0,
        });
        if let Ok(mut last) = self.1.lock() {
            *last = None;
        }
        Ok(status(&state))
    }
    pub fn status(&self) -> RecordingStatus {
        let Ok(mut state) = self.0.lock() else {
            return RecordingStatus {
                active: false,
                path: None,
                entries: 0,
                stopped_reason: Some("Запись недоступна: внутренняя ошибка"),
            };
        };
        self.check_limit(&mut state);
        if state.is_none() {
            if let Ok(last) = self.1.lock() {
                if let Some(last) = last.as_ref() {
                    return last.clone();
                }
            }
        }
        status(&state)
    }
    pub fn stop(&self) -> Result<RecordingStatus, String> {
        let mut state = self.0.lock().map_err(|_| "Запись недоступна")?;
        if state.is_none() {
            if let Ok(last) = self.1.lock() {
                if let Some(last) = last.as_ref() {
                    return Ok(last.clone());
                }
            }
        }
        let result = status(&state);
        if let Some(recording) = state.as_mut() {
            recording
                .file
                .sync_all()
                .map_err(|_| "Не удалось сохранить запись")?;
        }
        *state = None;
        let result = RecordingStatus {
            active: false,
            ..result
        };
        if let Ok(mut last) = self.1.lock() {
            *last = Some(result.clone());
        }
        Ok(result)
    }
    pub fn record(
        &self,
        kind: &str,
        method: &str,
        route: &str,
        status: Option<u16>,
        body: Option<&str>,
        data: &Value,
    ) {
        if !allowed_route(route) {
            return;
        }
        let Ok(mut state) = self.0.lock() else {
            return;
        };
        self.check_limit(&mut state);
        let Some(recording) = state.as_mut() else {
            return;
        };
        let request = body
            .and_then(|s| serde_json::from_str::<Value>(s).ok())
            .map(|v| sanitize(&v));
        let mut line = serde_json::to_vec(
            &serde_json::json!({"kind":kind,"at_ms":recording.start.elapsed().as_millis(),
            "method":method,"route":route,"status":status,"request":request,"data":sanitize(data)}),
        )
        .unwrap();
        line.push(b'\n');
        if line.len() > 512 * 1024 {
            self.remember_stop(recording, "Событие превышает допустимый размер");
            *state = None;
            return;
        }
        if recording.file.write_all(&line).is_err() {
            self.remember_stop(
                recording,
                "Не удалось записать файл: проверьте место и доступ к диску",
            );
            *state = None;
            return;
        }
        recording.bytes += line.len();
        recording.count += 1;
    }
    fn remember_stop(&self, recording: &Recording, reason: &'static str) {
        log::warn!("[SessionRecorder] {}", reason);
        if let Ok(mut last) = self.1.lock() {
            *last = Some(RecordingStatus {
                active: false,
                path: Some(recording.path.to_string_lossy().into_owned()),
                entries: recording.count,
                stopped_reason: Some(reason),
            });
        }
    }
    fn check_limit(&self, state: &mut Option<Recording>) {
        if let Some(recording) = state.as_ref() {
            if recording.bytes >= 20 * 1024 * 1024 || recording.start.elapsed().as_secs() >= 1800 {
                self.remember_stop(recording, "Достигнут лимит записи: 20 МБ или 30 минут");
                *state = None;
            }
        }
    }
}
fn status(state: &Option<Recording>) -> RecordingStatus {
    RecordingStatus {
        active: state.is_some(),
        path: state
            .as_ref()
            .map(|r| r.path.to_string_lossy().into_owned()),
        entries: state.as_ref().map_or(0, |r| r.count),
        stopped_reason: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn idle_recording_expires_without_needing_another_network_event() {
        let dir =
            std::env::temp_dir().join(format!("rustlm-idle-recording-{}", rand::random::<u128>()));
        let recorder = SessionRecorder::default();
        let original = recorder.start(dir.clone()).unwrap();
        recorder.0.lock().unwrap().as_mut().unwrap().start =
            Instant::now() - std::time::Duration::from_secs(1800);
        assert!(!recorder.active());
        let status = recorder.status();
        assert!(!status.active);
        assert_eq!(status.path, original.path);
        assert!(status.stopped_reason.unwrap().contains("лимит"));
        assert_eq!(recorder.stop().unwrap().path, original.path);
        assert!(recorder
            .start(dir.clone())
            .unwrap()
            .stopped_reason
            .is_none());
        recorder.stop().unwrap();
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn storage_quota_counts_only_recording_files_and_never_removes_them() {
        let dir = std::env::temp_dir().join(rand::random::<u128>().to_string());
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("unrelated.txt"), [0; 100]).unwrap();
        std::fs::create_dir(dir.join("lcu-directory.jsonl")).unwrap();
        assert!(storage_available(&dir, 10, 2).unwrap());
        let record = dir.join("lcu-test.jsonl");
        std::fs::write(&record, [0; 10]).unwrap();
        assert!(storage_available(&dir, 10, 2).unwrap());
        assert!(!storage_available(&dir, 9, 2).unwrap());
        assert!(!storage_available(&dir, 10, 1).unwrap());
        assert!(record.exists());
        assert_eq!(std::fs::read(&record).unwrap().len(), 10);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn automatic_stop_keeps_reason_and_path_until_next_start() {
        let dir = std::env::temp_dir().join(rand::random::<u128>().to_string());
        let recorder = SessionRecorder::default();
        let path = recorder.start(dir.clone()).unwrap().path;
        recorder.0.lock().unwrap().as_mut().unwrap().bytes = 20 * 1024 * 1024;
        recorder.record(
            "http",
            "GET",
            "/lol-gameflow/v1/session",
            Some(200),
            None,
            &Value::Null,
        );
        let stopped = recorder.status();
        assert!(!stopped.active);
        assert_eq!(stopped.path, path);
        assert!(stopped.stopped_reason.unwrap().contains("лимит"));
        assert_eq!(
            recorder.stop().unwrap().stopped_reason,
            stopped.stopped_reason
        );
        assert!(recorder
            .start(dir.clone())
            .unwrap()
            .stopped_reason
            .is_none());
        recorder.stop().unwrap();
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn write_failure_retains_diagnostic_instead_of_silently_stopping() {
        let dir = std::env::temp_dir().join(rand::random::<u128>().to_string());
        let recorder = SessionRecorder::default();
        let path = recorder.start(dir.clone()).unwrap().path.unwrap();
        recorder.0.lock().unwrap().as_mut().unwrap().file = File::open(&path).unwrap();
        recorder.record(
            "http",
            "GET",
            "/lol-gameflow/v1/session",
            Some(200),
            None,
            &Value::Null,
        );
        let stopped = recorder.status();
        assert!(!stopped.active);
        assert_eq!(stopped.entries, 0);
        assert_eq!(stopped.path.as_deref(), Some(path.as_str()));
        assert!(stopped.stopped_reason.unwrap().contains("диск"));
        assert_eq!(std::fs::read_to_string(path).unwrap().lines().count(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn credentials_names_and_unknown_fields_never_survive() {
        let clean = sanitize(
            &serde_json::json!({"username":"secret","password":"secret","myTeam":[{"cellId":1,"championId":99,"summonerName":"secret","puuid":"secret"}],"state":"private-name","phase":"ChampSelect"}),
        );
        assert!(!clean.to_string().contains("secret"));
        assert_eq!(clean["myTeam"][0]["championId"], 99);
        assert_eq!(clean["phase"], "ChampSelect");
        assert!(!allowed_route("/rso-auth/v1/session/credentials"));
        assert!(!allowed_route(
            "/lol-champ-select/v1/session/actions/1?token=secret"
        ));
    }
    #[test]
    fn recording_is_exclusive_and_stops_before_later_requests() {
        let dir = std::env::temp_dir().join(rand::random::<u128>().to_string());
        let recorder = SessionRecorder::default();
        let path = recorder.start(dir.clone()).unwrap().path.unwrap();
        assert!(recorder.start(dir.clone()).is_err());
        recorder.record(
            "http",
            "GET",
            "/lol-gameflow/v1/gameflow-phase",
            Some(200),
            None,
            &Value::String("Lobby".into()),
        );
        assert_eq!(recorder.stop().unwrap().entries, 1);
        recorder.record(
            "http",
            "GET",
            "/lol-gameflow/v1/gameflow-phase",
            Some(200),
            None,
            &Value::Null,
        );
        assert_eq!(std::fs::read_to_string(path).unwrap().lines().count(), 2);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
