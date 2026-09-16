//! Tra key + quyền model. Hot path: SHA-256 key -> HashMap lookup trong snapshot (lock-free).

use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};

use crate::contract::{ApiKey, ConfigSnapshot};

/// SHA-256 key plaintext -> [u8;32].
pub fn hash_key(plaintext: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(plaintext.as_bytes());
    let result = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&result);
    out
}

/// Lý do từ chối, giúp handler phân biệt 401/403.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthError {
    InvalidKey,
    Expired,
    Disabled,
    ModelNotAllowed,
}

/// Model có được phép cho key này không. allowed_models rỗng = cho phép tất cả.
pub fn model_allowed(key: &ApiKey, model: &str) -> bool {
    key.allowed_models.is_empty() || key.allowed_models.iter().any(|m| m == model)
}

/// Xác thực key (tồn tại, enabled, chưa hết hạn) — KHÔNG check model.
/// Dùng để 401 nhanh trước khi đọc body.
pub fn authorize_key(snapshot: &ConfigSnapshot, key_hash: &[u8; 32]) -> Result<ApiKey, AuthError> {
    let key = snapshot
        .keys_by_hash
        .get(key_hash)
        .ok_or(AuthError::InvalidKey)?;

    if !key.enabled {
        return Err(AuthError::Disabled);
    }

    // Team của key phải tồn tại và enabled — disable team là chặn toàn bộ key của team đó.
    let Some(team) = snapshot.teams.get(&key.team_id) else {
        return Err(AuthError::Disabled);
    };
    if !team.enabled {
        return Err(AuthError::Disabled);
    }

    if let Some(exp) = key.expires_at {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        if exp <= now {
            return Err(AuthError::Expired);
        }
    }

    Ok(key.clone())
}

/// Xác thực key + quyền model. Đúng key sai model -> ModelNotAllowed (handler trả 403).
pub fn authorize_detailed(
    snapshot: &ConfigSnapshot,
    key_hash: &[u8; 32],
    model: &str,
) -> Result<ApiKey, AuthError> {
    let key = authorize_key(snapshot, key_hash)?;
    if !model_allowed(&key, model) {
        return Err(AuthError::ModelNotAllowed);
    }
    Ok(key)
}

/// Trả ApiKey nếu key hợp lệ + model được phép.
pub fn authorize(snapshot: &ConfigSnapshot, key_hash: &[u8; 32], model: &str) -> Option<ApiKey> {
    authorize_detailed(snapshot, key_hash, model).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::{ApiKey, ConfigSnapshot, Team};
    use std::collections::HashMap;

    fn make_key(
        key_hash: [u8; 32],
        allowed_models: Vec<String>,
        enabled: bool,
        expires_at: Option<i64>,
    ) -> ApiKey {
        ApiKey {
            id: 1,
            key_hash,
            key_prefix: "test".into(),
            team_id: 1,
            owner: "tester".into(),
            allowed_models,
            budget: None,
            rpm_limit: None,
            concurrency_limit: None,
            expires_at,
            enabled,
        }
    }

    fn make_team(id: i64, enabled: bool) -> Team {
        Team {
            id,
            name: format!("team-{id}"),
            budget: None,
            enabled,
        }
    }

    fn snapshot(keys: HashMap<[u8; 32], ApiKey>, teams: HashMap<i64, Team>) -> ConfigSnapshot {
        ConfigSnapshot {
            keys_by_hash: keys,
            teams,
            ..Default::default()
        }
    }

    #[test]
    fn hash_is_sha256_hex32() {
        let expected: [u8; 32] = [
            0xba, 0x78, 0x16, 0xbf, 0x8f, 0x01, 0xcf, 0xea, 0x41, 0x41, 0x40, 0xde, 0x5d, 0xae,
            0x22, 0x23, 0xb0, 0x03, 0x61, 0xa3, 0x96, 0x17, 0x7a, 0x9c, 0xb4, 0x10, 0xff, 0x61,
            0xf2, 0x00, 0x15, 0xad,
        ];
        assert_eq!(hash_key("abc"), expected);
    }

    #[test]
    fn expired_key_rejected() {
        let key_hash = hash_key("secret");
        let key = make_key(key_hash, vec![], true, Some(0));
        let keys = HashMap::from([(key_hash, key)]);
        let teams = HashMap::from([(1, make_team(1, true))]);
        let snapshot = snapshot(keys, teams);

        assert!(authorize(&snapshot, &key_hash, "any-model").is_none());
        assert_eq!(
            authorize_detailed(&snapshot, &key_hash, "any-model"),
            Err(AuthError::Expired)
        );
    }

    #[test]
    fn wrong_model_403_vs_wrong_key_401() {
        let key_hash = hash_key("valid-key");
        let key = make_key(key_hash, vec!["model-a".to_string()], true, None);
        let keys = HashMap::from([(key_hash, key)]);
        let teams = HashMap::from([(1, make_team(1, true))]);
        let snapshot = snapshot(keys, teams);

        let wrong_hash = hash_key("wrong");
        assert_eq!(
            authorize_detailed(&snapshot, &wrong_hash, "model-a"),
            Err(AuthError::InvalidKey)
        );
        assert_eq!(
            authorize_detailed(&snapshot, &key_hash, "model-b"),
            Err(AuthError::ModelNotAllowed)
        );
        assert!(authorize(&snapshot, &key_hash, "model-a").is_some());
    }

    #[test]
    fn disabled_key_rejected() {
        let key_hash = hash_key("disabled");
        let key = make_key(key_hash, vec![], false, None);
        let keys = HashMap::from([(key_hash, key)]);
        let teams = HashMap::from([(1, make_team(1, true))]);
        let snapshot = snapshot(keys, teams);
        assert_eq!(
            authorize_detailed(&snapshot, &key_hash, "any-model"),
            Err(AuthError::Disabled)
        );
    }

    #[test]
    fn disabled_team_rejects_key() {
        let key_hash = hash_key("secret");
        let key = make_key(key_hash, vec![], true, None); // key enabled
        let keys = HashMap::from([(key_hash, key)]);
        let teams = HashMap::from([(1, make_team(1, false))]); // team disabled
        let snapshot = snapshot(keys, teams);

        assert_eq!(
            authorize_key(&snapshot, &key_hash),
            Err(AuthError::Disabled)
        );
        assert!(authorize(&snapshot, &key_hash, "any-model").is_none());
    }

    #[test]
    fn missing_team_rejects_key() {
        let key_hash = hash_key("secret");
        let key = make_key(key_hash, vec![], true, None); // key enabled, team absent
        let keys = HashMap::from([(key_hash, key)]);
        let snapshot = snapshot(keys, HashMap::new());

        assert_eq!(
            authorize_key(&snapshot, &key_hash),
            Err(AuthError::Disabled)
        );
        assert!(authorize(&snapshot, &key_hash, "any-model").is_none());
    }
}
