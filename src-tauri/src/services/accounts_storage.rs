use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;

use crate::error::AppError;
use crate::models::account::{
    AccountRecord, EncryptedExportData, ExportAccountRecord, LegacyExportAccountRecord,
};
use crate::services::crypto::{dpapi_protect, dpapi_unprotect};

#[derive(Default, Serialize, Deserialize)]
struct RemovedAccounts {
    #[serde(default)]
    usernames: BTreeSet<String>,
    #[serde(default)]
    profiles: BTreeSet<String>,
}
fn digest(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}
fn login_key(username: &str) -> String {
    digest(&format!("login:{}", username.trim().to_lowercase()))
}
fn region(server: &str) -> Option<&'static str> {
    match server.trim().to_ascii_uppercase().as_str() {
        "RU" | "RU1" => Some("RU"),
        "EUW" | "EUW1" => Some("EUW"),
        "EUNE" | "EUN1" => Some("EUNE"),
        "NA" | "NA1" => Some("NA"),
        "BR" | "BR1" => Some("BR"),
        "KR" => Some("KR"),
        "JP" | "JP1" => Some("JP"),
        "TR" | "TR1" => Some("TR"),
        "LAN" | "LA1" => Some("LAN"),
        "LAS" | "LA2" => Some("LAS"),
        "OCE" | "OC1" => Some("OCE"),
        _ => None,
    }
}
fn profile_key(account: &AccountRecord) -> Option<String> {
    let server = region(&account.server)?;
    let id = if account.riot_id.trim().is_empty() {
        account.summoner_name.trim()
    } else {
        account.riot_id.trim()
    }
    .to_lowercase();
    if !id.contains('#') {
        return None;
    }
    Some(digest(&format!("profile:{server}:{id}")))
}
fn placeholder(account: &AccountRecord) -> bool {
    account.username.starts_with("gl:") && account.encrypted_password.is_empty()
}
fn same_identity(a: &AccountRecord, b: &AccountRecord) -> bool {
    if !a.puuid.trim().is_empty() && !b.puuid.trim().is_empty() {
        return a.puuid.trim() == b.puuid.trim();
    }
    matches!((profile_key(a),profile_key(b)),(Some(a),Some(b)) if a==b)
}
impl RemovedAccounts {
    fn contains(&self, account: &AccountRecord) -> bool {
        self.usernames.contains(&login_key(&account.username))
            || (placeholder(account)
                && profile_key(account).is_some_and(|k| self.profiles.contains(&k)))
    }
    fn restore(&mut self, account: &AccountRecord) {
        self.usernames.remove(&login_key(&account.username));
        if let Some(key) = profile_key(account) {
            self.profiles.remove(&key);
        }
    }
}
fn deduplicate(accounts: &mut Vec<AccountRecord>) {
    for account in accounts.iter_mut() {
        if let Some(server) = region(&account.server) {
            account.server = server.into();
        }
    }
    let mut index = 0;
    while index < accounts.len() {
        if placeholder(&accounts[index]) {
            let target = accounts
                .iter()
                .enumerate()
                .find(|(other, a)| {
                    *other != index
                        && same_identity(&accounts[index], a)
                        && (!placeholder(a) || *other < index)
                })
                .map(|(i, _)| i);
            if let Some(target) = target {
                let duplicate = accounts[index].clone();
                let retained = &mut accounts[target];
                retained.pinned |= duplicate.pinned;
                if !duplicate.note.is_empty()
                    && duplicate.note != "Импорт из GoodLuck"
                    && !retained.note.contains(&duplicate.note)
                {
                    if !retained.note.is_empty() {
                        retained.note.push('\n');
                    }
                    retained.note.push_str(&duplicate.note);
                }
                accounts.remove(index);
                continue;
            }
        }
        index += 1;
    }
}

pub struct AccountsStorage {
    accounts_path: PathBuf,
    cached_accounts: Mutex<Option<Vec<AccountRecord>>>,
}

impl AccountsStorage {
    pub fn new() -> Self {
        let roaming_dir = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("LolManager");

        fs::create_dir_all(&roaming_dir).ok();

        let accounts_path = roaming_dir.join("accounts.json");

        // Migration from LocalAppData (old location)
        if !accounts_path.exists() {
            let old_path = dirs::data_local_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("LolManager")
                .join("accounts.json");
            if old_path.exists() {
                fs::copy(&old_path, &accounts_path).ok();
            }
        }

        Self {
            accounts_path,
            cached_accounts: Mutex::new(None),
        }
    }

    pub fn load_all(&self) -> Vec<AccountRecord> {
        let mut cache = self.cached_accounts.lock().unwrap();
        if let Some(accounts) = cache.as_ref() {
            return accounts.clone();
        }
        let loaded = (|| -> Result<Vec<AccountRecord>, AppError> {
            let mut accounts = self.read_from_disk()?;
            let before = serde_json::to_vec(&accounts)?;
            let removed = self.read_removed()?;
            accounts.retain(|a| !removed.contains(a));
            deduplicate(&mut accounts);
            if serde_json::to_vec(&accounts)? != before {
                self.write_to_disk(&accounts)?;
            }
            Ok(accounts)
        })();
        match loaded {
            Ok(accounts) => {
                *cache = Some(accounts.clone());
                accounts
            }
            Err(error) => {
                log::error!("Cannot read account storage: {}", error);
                Vec::new()
            }
        }
    }
    fn read_removed(&self) -> Result<RemovedAccounts, AppError> {
        let path = self.accounts_path.with_file_name("removed-accounts.json");
        if !path.exists() {
            return Ok(RemovedAccounts::default());
        }
        Ok(serde_json::from_slice(&fs::read(path)?)?)
    }
    fn edit<T>(
        &self,
        action: impl FnOnce(&mut Vec<AccountRecord>, &mut RemovedAccounts) -> Result<T, AppError>,
    ) -> Result<T, AppError> {
        let mut cache = self.cached_accounts.lock().unwrap();
        let mut accounts = match cache.as_ref() {
            Some(a) => a.clone(),
            None => self.read_from_disk()?,
        };
        let mut removed = self.read_removed()?;
        accounts.retain(|a| !removed.contains(a));
        let result = action(&mut accounts, &mut removed)?;
        deduplicate(&mut accounts);
        let bytes = serde_json::to_vec_pretty(&removed)?;
        super::network::atomic_write(
            &self.accounts_path.with_file_name("removed-accounts.json"),
            &bytes,
        )
        .map_err(AppError::Custom)?;
        self.write_to_disk(&accounts)?;
        *cache = Some(accounts);
        Ok(result)
    }
    pub fn save(&self, account: AccountRecord) -> Result<(), AppError> {
        self.edit(|accounts, removed| {
            removed.restore(&account);
            if let Some(existing) = accounts
                .iter_mut()
                .find(|a| a.username.eq_ignore_ascii_case(&account.username))
            {
                *existing = account;
            } else {
                accounts.push(account);
            }
            Ok(())
        })
    }
    pub fn update_existing(&self, account: AccountRecord) -> Result<bool, AppError> {
        self.edit(|accounts, _| {
            if let Some(existing) = accounts
                .iter_mut()
                .find(|a| a.username.eq_ignore_ascii_case(&account.username))
            {
                // LCU refreshes cannot overwrite credentials, notes or pins changed concurrently.
                existing.avatar_url = account.avatar_url;
                existing.summoner_name = account.summoner_name;
                existing.riot_id = account.riot_id;
                existing.puuid = account.puuid;
                existing.rank = account.rank;
                existing.rank_display = account.rank_display;
                existing.rank_icon_url = account.rank_icon_url;
                existing.server = account.server;
                existing.summoner_level = account.summoner_level;
                Ok(true)
            } else {
                Ok(false)
            }
        })
    }
    pub fn save_accounts(&self, ordered: Vec<AccountRecord>) -> Result<(), AppError> {
        // Ordering uses current records; a stale UI list must not restore a deleted account.
        self.edit(|accounts, _| {
            accounts.sort_by_key(|a| {
                ordered
                    .iter()
                    .position(|b| b.username == a.username)
                    .unwrap_or(usize::MAX)
            });
            Ok(())
        })
    }
    pub fn set_pinned(&self, username: &str, pinned: bool) -> Result<(), AppError> {
        self.edit(|accounts, _| {
            if let Some(account) = accounts
                .iter_mut()
                .find(|a| a.username.eq_ignore_ascii_case(username))
            {
                account.pinned = pinned;
            }
            Ok(())
        })
    }
    pub fn delete(&self, username: &str) -> Result<(), AppError> {
        self.edit(|accounts, removed| {
            removed.usernames.insert(login_key(username));
            if let Some(account) = accounts
                .iter()
                .find(|a| a.username.eq_ignore_ascii_case(username))
            {
                if let Some(key) = profile_key(account) {
                    removed.profiles.insert(key);
                }
            }
            accounts.retain(|a| !a.username.eq_ignore_ascii_case(username));
            Ok(())
        })
    }
    pub fn import_profiles(
        &self,
        profiles: Vec<crate::models::goodluck::GoodLuckRiotAccount>,
        restore_removed: bool,
    ) -> Result<(u32, u32), AppError> {
        self.edit(|accounts, removed| {
            let mut imported = 0;
            let mut skipped = 0;
            for profile in profiles {
                let record = AccountRecord {
                    username: format!(
                        "gl:{}:{}",
                        region(&profile.server)
                            .map(str::to_owned)
                            .unwrap_or_else(|| profile.server.clone()),
                        profile.riot_id
                    ),
                    note: "Импорт из GoodLuck".into(),
                    summoner_name: profile.riot_id.split('#').next().unwrap_or_default().into(),
                    riot_id: profile.riot_id,
                    server: profile.server,
                    rank: profile.rank.clone(),
                    rank_display: profile.rank,
                    ..Default::default()
                };
                if restore_removed {
                    removed.restore(&record);
                }
                if removed.contains(&record)
                    || accounts.iter().any(|a| {
                        a.username.eq_ignore_ascii_case(&record.username)
                            || same_identity(a, &record)
                    })
                {
                    skipped += 1;
                    continue;
                }
                accounts.push(record);
                imported += 1;
            }
            Ok((imported, skipped))
        })
    }

    pub fn load_all_with_passwords(&self) -> Vec<(AccountRecord, String)> {
        self.load_all()
            .into_iter()
            .map(|a| {
                let password = self.unprotect(&a.encrypted_password).unwrap_or_default();
                (a, password)
            })
            .collect()
    }

    pub fn import_from_cloud(
        &self,
        cloud_accounts: Vec<crate::models::account::CloudAccountData>,
    ) -> Result<usize, AppError> {
        self.edit(|accounts, removed| {
            let mut imported = 0;
            for ca in cloud_accounts {
                let record = AccountRecord {
                    username: ca.username,
                    encrypted_password: self.protect(&ca.password)?,
                    note: ca.note,
                    created_at: ca.created_at,
                    avatar_url: ca.avatar_url,
                    summoner_name: ca.summoner_name,
                    rank: ca.rank,
                    rank_display: ca.rank_display,
                    riot_id: ca.riot_id,
                    puuid: ca.puuid,
                    rank_icon_url: ca.rank_icon_url,
                    server: ca.server,
                    summoner_level: ca.summoner_level,
                    pinned: ca.pinned,
                    is_selected: false,
                };
                if removed.contains(&record) {
                    continue;
                }
                if let Some(existing) = accounts
                    .iter_mut()
                    .find(|a| a.username.eq_ignore_ascii_case(&record.username))
                {
                    let mut record = record;
                    if record.encrypted_password.is_empty() {
                        record.encrypted_password = existing.encrypted_password.clone();
                    }
                    *existing = record;
                } else {
                    accounts.push(record);
                    imported += 1;
                }
            }
            Ok(imported)
        })
    }

    pub fn protect(&self, plain: &str) -> Result<String, AppError> {
        if plain.is_empty() {
            return Ok(String::new());
        }
        dpapi_protect(plain.as_bytes())
    }

    pub fn unprotect(&self, encrypted: &str) -> Result<String, AppError> {
        if encrypted.is_empty() {
            return Ok(String::new());
        }
        dpapi_unprotect(encrypted)
    }

    pub fn export_accounts(
        &self,
        path: &str,
        password: Option<&str>,
        selected_usernames: Option<&[String]>,
    ) -> Result<(), AppError> {
        let accounts = self.load_all();
        let to_export: Vec<&AccountRecord> = match selected_usernames {
            Some(usernames) => accounts
                .iter()
                .filter(|a| usernames.contains(&a.username))
                .collect(),
            None => accounts.iter().collect(),
        };

        let export_records: Vec<ExportAccountRecord> = to_export
            .iter()
            .map(|a| {
                let password = self.unprotect(&a.encrypted_password).unwrap_or_default();
                ExportAccountRecord {
                    username: a.username.clone(),
                    password,
                    note: a.note.clone(),
                    created_at: a.created_at,
                    avatar_url: a.avatar_url.clone(),
                    summoner_name: a.summoner_name.clone(),
                    rank: a.rank.clone(),
                    rank_display: a.rank_display.clone(),
                    riot_id: a.riot_id.clone(),
                    puuid: a.puuid.clone(),
                    rank_icon_url: a.rank_icon_url.clone(),
                    summoner_level: a.summoner_level,
                    pinned: a.pinned,
                }
            })
            .collect();

        match password {
            Some(pwd) => {
                let json = serde_json::to_string(&export_records)?;
                let (encrypted, salt, iv) = aes_encrypt(json.as_bytes(), pwd)?;
                let export_data = EncryptedExportData {
                    version: 3,
                    app_name: "LolManager".to_string(),
                    exported_at: chrono::Utc::now(),
                    encrypted_accounts: encrypted,
                    salt,
                    iv: Some(iv),
                };
                let content = serde_json::to_string_pretty(&export_data)?;
                fs::write(path, content)?;
            }
            None => {
                let content = serde_json::to_string_pretty(&export_records)?;
                fs::write(path, content)?;
            }
        }
        Ok(())
    }

    pub fn import_accounts(&self, path: &str, password: Option<&str>) -> Result<usize, AppError> {
        let content = fs::read_to_string(path)?;

        let import_records: Vec<ExportAccountRecord> = if let Ok(export_data) =
            serde_json::from_str::<EncryptedExportData>(&content)
        {
            if export_data.version == 3 && !export_data.encrypted_accounts.is_empty() {
                // Encrypted export (v3)
                let pwd = password.ok_or_else(|| {
                    AppError::Custom("Password required for encrypted import".to_string())
                })?;
                let iv = export_data
                    .iv
                    .ok_or_else(|| AppError::Custom("Missing IV in export data".to_string()))?;
                let decrypted =
                    aes_decrypt(&export_data.encrypted_accounts, &export_data.salt, &iv, pwd)?;
                serde_json::from_slice(&decrypted)?
            } else if let Ok(records) = serde_json::from_str::<Vec<ExportAccountRecord>>(&content) {
                records
            } else {
                return Err(AppError::Custom("Unknown export format".to_string()));
            }
        } else if let Ok(records) = serde_json::from_str::<Vec<ExportAccountRecord>>(&content) {
            // Plain export
            records
        } else if let Ok(legacy) = serde_json::from_str::<Vec<LegacyExportAccountRecord>>(&content)
        {
            // Legacy v1
            legacy
                .into_iter()
                .map(|l| ExportAccountRecord {
                    username: l.username,
                    password: l.password,
                    note: String::new(),
                    created_at: l.created_at,
                    avatar_url: String::new(),
                    summoner_name: String::new(),
                    rank: String::new(),
                    rank_display: String::new(),
                    riot_id: String::new(),
                    puuid: String::new(),
                    rank_icon_url: String::new(),
                    summoner_level: 0,
                    pinned: false,
                })
                .collect()
        } else {
            return Err(AppError::Custom("Unknown export format".to_string()));
        };

        let mut prepared = Vec::new();
        for record in import_records {
            let encrypted_password = self.protect(&record.password)?;
            let new_account = AccountRecord {
                username: record.username.clone(),
                encrypted_password,
                note: record.note,
                created_at: record.created_at,
                avatar_url: record.avatar_url,
                summoner_name: record.summoner_name,
                rank: record.rank,
                rank_display: record.rank_display,
                riot_id: record.riot_id,
                puuid: record.puuid,
                rank_icon_url: record.rank_icon_url,
                server: String::new(),
                summoner_level: record.summoner_level,
                pinned: record.pinned,
                is_selected: false,
            };

            prepared.push(new_account);
        }
        self.edit(|accounts, removed| {
            let mut imported = 0;
            for account in prepared {
                removed.restore(&account);
                if !accounts
                    .iter()
                    .any(|a| a.username.eq_ignore_ascii_case(&account.username))
                {
                    accounts.push(account);
                    imported += 1;
                }
            }
            Ok(imported)
        })
    }

    fn read_from_disk(&self) -> Result<Vec<AccountRecord>, AppError> {
        if !self.accounts_path.exists() {
            return Ok(Vec::new());
        }

        let content = fs::read_to_string(&self.accounts_path)?;
        if content.trim().is_empty() {
            return Ok(Vec::new());
        }

        let accounts: Vec<AccountRecord> = serde_json::from_str(&content)?;
        Ok(accounts)
    }

    fn write_to_disk(&self, accounts: &[AccountRecord]) -> Result<(), AppError> {
        // Create backup
        let backup_path = self.accounts_path.with_extension("json.bak");
        if self.accounts_path.exists() {
            fs::copy(&self.accounts_path, &backup_path)?;
        }

        let content = serde_json::to_string_pretty(accounts)?;
        super::network::atomic_write(&self.accounts_path, content.as_bytes())
            .map_err(AppError::Custom)?;
        Ok(())
    }
}

// --- AES-256-CBC encryption (for export/import) ---

fn aes_encrypt(data: &[u8], password: &str) -> Result<(String, String, String), AppError> {
    use aes::cipher::{BlockEncryptMut, KeyIvInit};
    use rand::RngCore;

    let mut salt = [0u8; 32];
    let mut iv = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut salt);
    rand::thread_rng().fill_bytes(&mut iv);

    // Derive key using PBKDF2-SHA256 (100000 iterations)
    let mut key = [0u8; 32];
    pbkdf2::pbkdf2_hmac::<sha2::Sha256>(password.as_bytes(), &salt, 100_000, &mut key);

    // PKCS7 padding
    let block_size = 16;
    let padding_len = block_size - (data.len() % block_size);
    let mut buf = data.to_vec();
    buf.extend(std::iter::repeat(padding_len as u8).take(padding_len));

    // Encrypt in-place using CBC
    type Aes256CbcEnc = cbc::Encryptor<aes::Aes256>;
    let encryptor = Aes256CbcEnc::new_from_slices(&key, &iv)
        .map_err(|e| AppError::Custom(format!("AES init failed: {}", e)))?;

    let buf_len = buf.len();
    let encrypted = encryptor
        .encrypt_padded_mut::<aes::cipher::block_padding::NoPadding>(&mut buf, buf_len)
        .map_err(|e| AppError::Custom(format!("AES encrypt failed: {}", e)))?;

    Ok((
        BASE64.encode(encrypted),
        BASE64.encode(&salt),
        BASE64.encode(&iv),
    ))
}

fn aes_decrypt(
    encrypted_b64: &str,
    salt_b64: &str,
    iv_b64: &str,
    password: &str,
) -> Result<Vec<u8>, AppError> {
    use aes::cipher::{BlockDecryptMut, KeyIvInit};

    let mut encrypted = BASE64
        .decode(encrypted_b64)
        .map_err(|e| AppError::Custom(format!("Base64 decode failed: {}", e)))?;
    let salt = BASE64
        .decode(salt_b64)
        .map_err(|e| AppError::Custom(format!("Base64 decode salt: {}", e)))?;
    let iv = BASE64
        .decode(iv_b64)
        .map_err(|e| AppError::Custom(format!("Base64 decode iv: {}", e)))?;

    // Derive key
    let mut key = [0u8; 32];
    pbkdf2::pbkdf2_hmac::<sha2::Sha256>(password.as_bytes(), &salt, 100_000, &mut key);

    // Decrypt in-place
    type Aes256CbcDec = cbc::Decryptor<aes::Aes256>;
    let decryptor = Aes256CbcDec::new_from_slices(&key, &iv)
        .map_err(|e| AppError::Custom(format!("AES init failed: {}", e)))?;

    let decrypted = decryptor
        .decrypt_padded_mut::<aes::cipher::block_padding::Pkcs7>(&mut encrypted)
        .map_err(|e| AppError::Custom(format!("AES decrypt failed: {}", e)))?;

    Ok(decrypted.to_vec())
}

#[cfg(test)]
mod storage_tests {
    use super::*;
    #[test]
    fn stale_profile_refresh_preserves_new_credentials_notes_and_pin() {
        let store = storage();
        let old = AccountRecord {
            username: "local-login".into(),
            encrypted_password: "old-ciphertext".into(),
            ..Default::default()
        };
        store.save(old.clone()).unwrap();
        let mut current = old.clone();
        current.encrypted_password = "new-ciphertext".into();
        current.note = "keep note".into();
        current.pinned = true;
        store.save(current).unwrap();
        store.update_existing(old).unwrap();
        let saved = store.load_all().remove(0);
        assert_eq!(saved.encrypted_password, "new-ciphertext");
        assert_eq!(saved.note, "keep note");
        assert!(saved.pinned);
        cleanup(store);
    }
    #[test]
    fn concurrent_cloud_pull_and_delete_leave_the_account_deleted() {
        let store = std::sync::Arc::new(storage());
        store
            .save(AccountRecord {
                username: "local-login".into(),
                ..Default::default()
            })
            .unwrap();
        let gate = std::sync::Arc::new(std::sync::Barrier::new(3));
        let deleting = store.clone();
        let deleting_gate = gate.clone();
        let deletion = std::thread::spawn(move || {
            deleting_gate.wait();
            deleting.delete("local-login").unwrap();
        });
        let syncing = store.clone();
        let syncing_gate = gate.clone();
        let sync = std::thread::spawn(move || {
            syncing_gate.wait();
            syncing
                .import_from_cloud(vec![serde_json::from_value(
                    serde_json::json!({"username":"local-login"}),
                )
                .unwrap()])
                .unwrap();
        });
        gate.wait();
        deletion.join().unwrap();
        sync.join().unwrap();
        assert!(store.load_all().is_empty());
        cleanup(std::sync::Arc::try_unwrap(store).ok().unwrap());
    }
    #[test]
    fn empty_cloud_password_does_not_erase_local_credentials() {
        let store = storage();
        store
            .save(AccountRecord {
                username: "local-login".into(),
                encrypted_password: "opaque-test-ciphertext".into(),
                ..Default::default()
            })
            .unwrap();
        store
            .import_from_cloud(vec![serde_json::from_value(
                serde_json::json!({"username":"local-login","password":""}),
            )
            .unwrap()])
            .unwrap();
        assert_eq!(
            store.load_all()[0].encrypted_password,
            "opaque-test-ciphertext"
        );
        cleanup(store);
    }
    fn storage() -> AccountsStorage {
        let root = std::env::temp_dir().join(format!("rustlm-accounts-{}", rand::random::<u64>()));
        fs::create_dir(&root).unwrap();
        AccountsStorage {
            accounts_path: root.join("accounts.json"),
            cached_accounts: Mutex::new(None),
        }
    }
    fn profile() -> crate::models::goodluck::GoodLuckRiotAccount {
        crate::models::goodluck::GoodLuckRiotAccount {
            riot_id: "Player#RU1".into(),
            server: "RU".into(),
            rank: "GOLD".into(),
        }
    }
    fn cleanup(storage: AccountsStorage) {
        fs::remove_dir_all(storage.accounts_path.parent().unwrap()).unwrap();
    }
    #[test]
    fn deletion_survives_restart_and_automatic_profile_import_but_explicit_readd_works() {
        let first = storage();
        assert_eq!(
            first.import_profiles(vec![profile()], false).unwrap(),
            (1, 0)
        );
        let username = first.load_all()[0].username.clone();
        first.delete(&username).unwrap();
        let reopened = AccountsStorage {
            accounts_path: first.accounts_path.clone(),
            cached_accounts: Mutex::new(None),
        };
        assert_eq!(
            reopened.import_profiles(vec![profile()], false).unwrap(),
            (0, 1)
        );
        assert!(reopened.load_all().is_empty());
        assert_eq!(
            reopened.import_profiles(vec![profile()], true).unwrap(),
            (1, 0)
        );
        assert_eq!(reopened.load_all().len(), 1);
        cleanup(first);
    }
    #[test]
    fn stale_cloud_order_and_background_update_never_restore_a_deleted_account() {
        let store = storage();
        let record = AccountRecord {
            username: "local-login".into(),
            riot_id: "Player#RU1".into(),
            server: "RU".into(),
            ..Default::default()
        };
        store.save(record.clone()).unwrap();
        store.delete("LOCAL-LOGIN").unwrap();
        let cloud = serde_json::from_value(
            serde_json::json!({"username":"local-login","riotId":"Player#RU1","server":"RU"}),
        )
        .unwrap();
        store.import_from_cloud(vec![cloud]).unwrap();
        assert!(!store.update_existing(record.clone()).unwrap());
        store.save_accounts(vec![record.clone()]).unwrap();
        assert!(store.load_all().is_empty());
        store.save(record).unwrap();
        assert_eq!(store.load_all().len(), 1);
        cleanup(store);
    }
    #[test]
    fn placeholder_duplicates_merge_into_credentials_and_region_aliases_match() {
        let store = storage();
        let real = AccountRecord {
            username: "real-login".into(),
            encrypted_password: "opaque-test-ciphertext".into(),
            riot_id: "Player#TAG".into(),
            server: "EUW".into(),
            note: "main".into(),
            ..Default::default()
        };
        let placeholder = AccountRecord {
            username: "gl:Player#TAG".into(),
            riot_id: "player#tag".into(),
            server: "EUW1".into(),
            note: "keep this note".into(),
            pinned: true,
            ..Default::default()
        };
        fs::write(
            &store.accounts_path,
            serde_json::to_vec(&vec![placeholder, real.clone()]).unwrap(),
        )
        .unwrap();
        let accounts = store.load_all();
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].username, real.username);
        assert_eq!(accounts[0].encrypted_password, real.encrypted_password);
        assert!(accounts[0].note.contains("main") && accounts[0].note.contains("keep this note"));
        assert!(accounts[0].pinned);
        assert!(store.accounts_path.with_extension("json.bak").is_file());
        cleanup(store);
    }
    #[test]
    fn different_regions_and_different_puuids_are_not_merged() {
        let mut a = AccountRecord {
            username: "gl:a".into(),
            riot_id: "Player#TAG".into(),
            server: "RU".into(),
            ..Default::default()
        };
        let mut b = AccountRecord {
            username: "real".into(),
            riot_id: "Player#TAG".into(),
            server: "EUW".into(),
            encrypted_password: "opaque".into(),
            ..Default::default()
        };
        assert!(!same_identity(&a, &b));
        a.server = "EUW1".into();
        a.puuid = "id-a".into();
        b.puuid = "id-b".into();
        assert!(!same_identity(&a, &b));
        let mut accounts = vec![a, b];
        deduplicate(&mut accounts);
        assert_eq!(accounts.len(), 2);
    }
    #[test]
    fn corrupt_storage_is_never_replaced_by_an_empty_cached_list() {
        let store = storage();
        fs::write(&store.accounts_path, b"broken-json").unwrap();
        assert!(store.load_all().is_empty());
        assert!(store
            .save(AccountRecord {
                username: "test".into(),
                ..Default::default()
            })
            .is_err());
        assert_eq!(fs::read(&store.accounts_path).unwrap(), b"broken-json");
        cleanup(store);
    }
}
