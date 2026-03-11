use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;

use crate::error::AppError;
use crate::models::account::{
    AccountRecord, EncryptedExportData, ExportAccountRecord, LegacyExportAccountRecord,
};

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
        if let Some(ref accounts) = *cache {
            return accounts.clone();
        }

        let accounts = self.read_from_disk().unwrap_or_default();
        *cache = Some(accounts.clone());
        accounts
    }

    pub fn save(&self, account: AccountRecord) -> Result<(), AppError> {
        let mut accounts = self.load_all();

        if let Some(existing) = accounts
            .iter_mut()
            .find(|a| a.username == account.username)
        {
            *existing = account;
        } else {
            accounts.push(account);
        }

        self.write_to_disk(&accounts)?;
        let mut cache = self.cached_accounts.lock().unwrap();
        *cache = Some(accounts);
        Ok(())
    }

    pub fn save_accounts(&self, accounts: Vec<AccountRecord>) -> Result<(), AppError> {
        self.write_to_disk(&accounts)?;
        let mut cache = self.cached_accounts.lock().unwrap();
        *cache = Some(accounts);
        Ok(())
    }

    pub fn delete(&self, username: &str) -> Result<(), AppError> {
        let mut accounts = self.load_all();
        accounts.retain(|a| a.username != username);
        self.write_to_disk(&accounts)?;
        let mut cache = self.cached_accounts.lock().unwrap();
        *cache = Some(accounts);
        Ok(())
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
                    rank_icon_url: a.rank_icon_url.clone(),
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

    pub fn import_accounts(
        &self,
        path: &str,
        password: Option<&str>,
    ) -> Result<usize, AppError> {
        let content = fs::read_to_string(path)?;

        let import_records: Vec<ExportAccountRecord> =
            if let Ok(export_data) = serde_json::from_str::<EncryptedExportData>(&content) {
                if export_data.version == 3 && !export_data.encrypted_accounts.is_empty() {
                    // Encrypted export (v3)
                    let pwd = password.ok_or_else(|| {
                        AppError::Custom("Password required for encrypted import".to_string())
                    })?;
                    let iv = export_data.iv.ok_or_else(|| {
                        AppError::Custom("Missing IV in export data".to_string())
                    })?;
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
            } else if let Ok(legacy) =
                serde_json::from_str::<Vec<LegacyExportAccountRecord>>(&content)
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
                        rank_icon_url: String::new(),
                    })
                    .collect()
            } else {
                return Err(AppError::Custom("Unknown export format".to_string()));
            };

        let mut accounts = self.load_all();
        let mut imported = 0;

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
                rank_icon_url: record.rank_icon_url,
                server: String::new(),
                is_selected: false,
            };

            if !accounts.iter().any(|a| a.username == record.username) {
                accounts.push(new_account);
                imported += 1;
            }
        }

        self.write_to_disk(&accounts)?;
        let mut cache = self.cached_accounts.lock().unwrap();
        *cache = Some(accounts);
        Ok(imported)
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
            fs::copy(&self.accounts_path, &backup_path).ok();
        }

        let content = serde_json::to_string_pretty(accounts)?;
        fs::write(&self.accounts_path, content)?;
        Ok(())
    }
}

// --- DPAPI (Windows only) ---

#[cfg(windows)]
fn dpapi_protect(data: &[u8]) -> Result<String, AppError> {
    use windows::Win32::Security::Cryptography::*;

    unsafe {
        let input_blob = CRYPT_INTEGER_BLOB {
            cbData: data.len() as u32,
            pbData: data.as_ptr() as *mut u8,
        };
        let mut output_blob = CRYPT_INTEGER_BLOB::default();

        CryptProtectData(
            &input_blob,
            None,
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output_blob,
        )
        .map_err(|e| AppError::Custom(format!("DPAPI Protect failed: {}", e)))?;

        let protected = std::slice::from_raw_parts(
            output_blob.pbData,
            output_blob.cbData as usize,
        )
        .to_vec();

        Ok(BASE64.encode(&protected))
    }
}

#[cfg(not(windows))]
fn dpapi_protect(data: &[u8]) -> Result<String, AppError> {
    // Fallback: base64 encode (not secure, but allows compilation on non-Windows)
    Ok(BASE64.encode(data))
}

#[cfg(windows)]
fn dpapi_unprotect(encrypted_b64: &str) -> Result<String, AppError> {
    use windows::Win32::Security::Cryptography::*;

    let protected_bytes = BASE64
        .decode(encrypted_b64)
        .map_err(|e| AppError::Custom(format!("Base64 decode failed: {}", e)))?;

    unsafe {
        let input_blob = CRYPT_INTEGER_BLOB {
            cbData: protected_bytes.len() as u32,
            pbData: protected_bytes.as_ptr() as *mut u8,
        };
        let mut output_blob = CRYPT_INTEGER_BLOB::default();

        CryptUnprotectData(
            &input_blob,
            None,
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output_blob,
        )
        .map_err(|e| AppError::Custom(format!("DPAPI Unprotect failed: {}", e)))?;

        let decrypted = std::slice::from_raw_parts(
            output_blob.pbData,
            output_blob.cbData as usize,
        )
        .to_vec();

        String::from_utf8(decrypted)
            .map_err(|e| AppError::Custom(format!("UTF-8 decode failed: {}", e)))
    }
}

#[cfg(not(windows))]
fn dpapi_unprotect(encrypted_b64: &str) -> Result<String, AppError> {
    let bytes = BASE64
        .decode(encrypted_b64)
        .map_err(|e| AppError::Custom(format!("Base64 decode failed: {}", e)))?;
    String::from_utf8(bytes).map_err(|e| AppError::Custom(format!("UTF-8 decode failed: {}", e)))
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
