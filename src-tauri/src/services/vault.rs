use super::crypto::{dpapi_protect, dpapi_unprotect};
use crate::error::AppError;
use aes_gcm::{
    aead::{Aead, Payload},
    Aes256Gcm, KeyInit,
};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

#[derive(Serialize, Deserialize, Clone)]
pub struct Envelope {
    pub version: u32,
    pub nonce: String,
    pub ciphertext: String,
}
#[derive(Serialize, Deserialize, Default)]
pub struct LocalVault {
    #[serde(default)]
    pub pending_key: Option<String>,
    pub key: String,
    pub revision: Option<String>,
    pub baseline: Option<Value>,
}
fn error() -> AppError {
    AppError::Custom(
        "Хранилище не удалось расшифровать: проверьте код подключения и аккаунт GoodLuck".into(),
    )
}
pub fn path(user: &str) -> std::path::PathBuf {
    dirs::data_local_dir()
        .unwrap_or_default()
        .join("LolManager")
        .join(format!("vault-{:x}.json", Sha256::digest(user.as_bytes())))
}
pub fn load(user: &str) -> Result<LocalVault, AppError> {
    if !path(user).exists() {
        return Err(AppError::Custom(
            "Подключите зашифрованное хранилище в настройках LM".into(),
        ));
    }
    let protected = std::fs::read_to_string(path(user))?;
    Ok(serde_json::from_str(&dpapi_unprotect(&protected)?)?)
}
pub fn save(user: &str, state: &LocalVault) -> Result<(), AppError> {
    let protected = dpapi_protect(&serde_json::to_vec(state)?)?;
    super::network::atomic_write(&path(user), protected.as_bytes()).map_err(AppError::Custom)
}
pub fn new_key() -> String {
    let mut key = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut key);
    B64.encode(key)
}
fn cipher(key: &str) -> Result<Aes256Gcm, AppError> {
    let decoded = B64.decode(key.trim()).map_err(|_| error())?;
    Aes256Gcm::new_from_slice(&decoded).map_err(|_| error())
}
pub fn seal(user: &str, key: &str, value: &Value) -> Result<Envelope, AppError> {
    let mut nonce = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce);
    let aad = format!("RustLM/vault/v1/{user}");
    let ciphertext = cipher(key)?
        .encrypt(
            (&nonce).into(),
            Payload {
                msg: &serde_json::to_vec(value)?,
                aad: aad.as_bytes(),
            },
        )
        .map_err(|_| error())?;
    Ok(Envelope {
        version: 1,
        nonce: B64.encode(nonce),
        ciphertext: B64.encode(ciphertext),
    })
}
pub fn open(user: &str, key: &str, envelope: &Envelope) -> Result<Value, AppError> {
    if envelope.version != 1 {
        return Err(error());
    }
    let nonce = B64.decode(&envelope.nonce).map_err(|_| error())?;
    let nonce: [u8; 12] = nonce.try_into().map_err(|_| error())?;
    let data = B64.decode(&envelope.ciphertext).map_err(|_| error())?;
    let aad = format!("RustLM/vault/v1/{user}");
    let plaintext = cipher(key)?
        .decrypt(
            (&nonce).into(),
            Payload {
                msg: &data,
                aad: aad.as_bytes(),
            },
        )
        .map_err(|_| error())?;
    Ok(serde_json::from_slice(&plaintext)?)
}
// Missing entries are deletions. Conflicting changes never silently overwrite another PC.
pub fn merge(base: &Value, local: &Value, remote: &Value) -> Result<Value, AppError> {
    merge_with_choice(base, local, remote, None)
}
pub fn merge_with_choice(
    base: &Value,
    local: &Value,
    remote: &Value,
    prefer_local: Option<bool>,
) -> Result<Value, AppError> {
    if local == remote {
        return Ok(local.clone());
    }
    if local == base {
        return Ok(remote.clone());
    }
    if remote == base {
        return Ok(local.clone());
    }
    if let (Some(b), Some(l), Some(r)) = (base.as_object(), local.as_object(), remote.as_object()) {
        let keys: std::collections::BTreeSet<_> =
            b.keys().chain(l.keys()).chain(r.keys()).collect();
        let mut result = serde_json::Map::new();
        for key in keys {
            let value = merge_with_choice(
                b.get(key).unwrap_or(&Value::Null),
                l.get(key).unwrap_or(&Value::Null),
                r.get(key).unwrap_or(&Value::Null),
                prefer_local,
            )?;
            if !value.is_null() {
                result.insert(key.clone(), value);
            }
        }
        return Ok(Value::Object(result));
    }
    if let Some(local_wins) = prefer_local {
        return Ok(if local_wins {
            local.clone()
        } else {
            remote.clone()
        });
    }
    Err(AppError::Custom("SYNC_CONFLICT: оба ПК изменили одну запись; выберите данные для совпавших записей в настройках".into()))
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn first_join_and_conflict_choice_keep_accounts_unique_to_either_pc() {
        let base = json!({"accounts":{}});
        let local = json!({"accounts":{"home":{"password":"home"},"shared":{"password":"new"}}});
        let remote = json!({"accounts":{"work":{"password":"work"},"shared":{"password":"old"}}});
        assert!(merge(&base, &local, &remote).is_err());
        let chosen = merge_with_choice(&base, &local, &remote, Some(true)).unwrap();
        assert_eq!(chosen["accounts"].as_object().unwrap().len(), 3);
        assert_eq!(chosen["accounts"]["shared"]["password"], "new");
        assert_eq!(
            merge(
                &base,
                &json!({"accounts":{"home":1}}),
                &json!({"accounts":{"work":2}})
            )
            .unwrap(),
            json!({"accounts":{"home":1,"work":2}})
        );
    }
    #[test]
    fn authenticated_cipher_rejects_wrong_user_key_and_tampering() {
        let key = new_key();
        let data = json!({"password":"test-secret"});
        let e = seal("alice", &key, &data).unwrap();
        assert_eq!(open("alice", &key, &e).unwrap(), data);
        assert!(!serde_json::to_string(&e).unwrap().contains("test-secret"));
        assert!(open("bob", &key, &e).is_err());
        assert!(open("alice", &new_key(), &e).is_err());
        let mut damaged = e.clone();
        damaged.ciphertext.replace_range(0..1, "!");
        assert!(open("alice", &key, &damaged).is_err());
        assert_ne!(seal("alice", &key, &data).unwrap().nonce, e.nonce);
    }
    #[test]
    fn merge_propagates_deletion_preserves_independent_changes_and_rejects_conflicts() {
        let b = json!({"a":{"password":"old"},"b":{"note":"old"}});
        let l = json!({"b":{"note":"old"}});
        let r = json!({"a":{"password":"old"},"b":{"note":"new"}});
        assert_eq!(merge(&b, &l, &r).unwrap(), json!({"b":{"note":"new"}}));
        assert!(merge(
            &json!({"p":"old"}),
            &json!({"p":"one"}),
            &json!({"p":"two"})
        )
        .is_err());
    }
}
