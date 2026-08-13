use std::collections::BTreeMap;
use std::num::NonZeroU32;

use ring::aead::{self, Aad, LessSafeKey, Nonce, UnboundKey};
use ring::digest::{SHA256, digest};
use ring::pbkdf2;
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};

const SYNC_PROTOCOL_VERSION: u8 = 1;
const ENCRYPTION_PROTOCOL: &str = "orbit-sync-aes256gcm-v1";
const PBKDF2_ITERATIONS: u32 = 120_000;
const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncEvent {
    pub event_id: String,
    pub device_id: String,
    pub sequence: i64,
    pub occurred_at_ms: i64,
    pub entity_kind: String,
    pub entity_id: String,
    pub operation: String,
    pub payload_json: String,
    pub payload_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncBundle {
    pub protocol_version: u8,
    pub source_device_id: String,
    pub exported_at_ms: i64,
    pub events: Vec<SyncEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatus {
    pub device_id: String,
    pub known_device_count: i64,
    pub event_count: i64,
    pub last_event_at_ms: Option<i64>,
    pub encryption_available: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncImportResult {
    pub inserted_event_count: usize,
    pub bundle_event_count: usize,
    pub source_device_id: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "storage", rename_all = "snake_case")]
enum StoredSyncBundle {
    Plain {
        bundle: SyncBundle,
    },
    Encrypted {
        protocol: String,
        iterations: u32,
        salt_hex: String,
        nonce_hex: String,
        ciphertext_hex: String,
    },
}

pub fn random_device_id() -> Result<String, String> {
    let mut bytes = [0_u8; 16];
    SystemRandom::new()
        .fill(&mut bytes)
        .map_err(|_| "secure random device ID generation failed".to_string())?;
    Ok(format!("device-{}", hex(&bytes)))
}

pub fn build_sync_event(
    device_id: &str,
    sequence: i64,
    occurred_at_ms: i64,
    entity_kind: &str,
    entity_id: &str,
    operation: &str,
    payload_json: &str,
) -> Result<SyncEvent, String> {
    if device_id.trim().is_empty()
        || entity_kind.trim().is_empty()
        || entity_id.trim().is_empty()
        || operation.trim().is_empty()
    {
        return Err("sync event identity fields must not be empty".to_string());
    }
    let payload_value: serde_json::Value = serde_json::from_str(payload_json)
        .map_err(|error| format!("sync payload is not valid JSON: {error}"))?;
    let canonical_payload = serde_json::to_string(&payload_value)
        .map_err(|error| format!("sync payload cannot be serialized: {error}"))?;
    let payload_hash = sha256_hex(canonical_payload.as_bytes());
    let identity = format!(
        "{device_id}\n{sequence}\n{occurred_at_ms}\n{entity_kind}\n{entity_id}\n{operation}\n{payload_hash}"
    );
    Ok(SyncEvent {
        event_id: format!("event-{}", sha256_hex(identity.as_bytes())),
        device_id: device_id.to_string(),
        sequence,
        occurred_at_ms,
        entity_kind: entity_kind.to_string(),
        entity_id: entity_id.to_string(),
        operation: operation.to_string(),
        payload_json: canonical_payload,
        payload_hash,
    })
}

pub fn validate_sync_event(event: &SyncEvent) -> Result<(), String> {
    let rebuilt = build_sync_event(
        &event.device_id,
        event.sequence,
        event.occurred_at_ms,
        &event.entity_kind,
        &event.entity_id,
        &event.operation,
        &event.payload_json,
    )?;
    if rebuilt.payload_hash != event.payload_hash || rebuilt.event_id != event.event_id {
        return Err(format!(
            "sync event {} failed integrity validation",
            event.event_id
        ));
    }
    Ok(())
}

pub fn merge_sync_events(
    local: impl IntoIterator<Item = SyncEvent>,
    remote: impl IntoIterator<Item = SyncEvent>,
) -> Result<Vec<SyncEvent>, String> {
    let mut by_id = BTreeMap::<String, SyncEvent>::new();
    for event in local.into_iter().chain(remote) {
        validate_sync_event(&event)?;
        match by_id.get(&event.event_id) {
            Some(existing) if existing != &event => {
                return Err(format!("sync event ID collision: {}", event.event_id));
            }
            Some(_) => {}
            None => {
                by_id.insert(event.event_id.clone(), event);
            }
        }
    }
    let mut merged = by_id.into_values().collect::<Vec<_>>();
    merged.sort_by(|left, right| {
        (
            left.occurred_at_ms,
            left.device_id.as_str(),
            left.sequence,
            left.event_id.as_str(),
        )
            .cmp(&(
                right.occurred_at_ms,
                right.device_id.as_str(),
                right.sequence,
                right.event_id.as_str(),
            ))
    });
    Ok(merged)
}

pub fn latest_entity_events(events: &[SyncEvent]) -> BTreeMap<(String, String), SyncEvent> {
    let mut latest = BTreeMap::new();
    for event in events {
        let key = (event.entity_kind.clone(), event.entity_id.clone());
        let replace = latest.get(&key).is_none_or(|current: &SyncEvent| {
            (
                event.occurred_at_ms,
                event.device_id.as_str(),
                event.sequence,
                event.event_id.as_str(),
            ) > (
                current.occurred_at_ms,
                current.device_id.as_str(),
                current.sequence,
                current.event_id.as_str(),
            )
        });
        if replace {
            latest.insert(key, event.clone());
        }
    }
    latest
}

pub fn make_sync_bundle(
    source_device_id: impl Into<String>,
    exported_at_ms: i64,
    events: Vec<SyncEvent>,
) -> Result<SyncBundle, String> {
    let source_device_id = source_device_id.into();
    let events = merge_sync_events(events, [])?;
    Ok(SyncBundle {
        protocol_version: SYNC_PROTOCOL_VERSION,
        source_device_id,
        exported_at_ms,
        events,
    })
}

pub fn encode_sync_bundle(
    bundle: &SyncBundle,
    passphrase: Option<&str>,
) -> Result<Vec<u8>, String> {
    if bundle.protocol_version != SYNC_PROTOCOL_VERSION {
        return Err("unsupported sync bundle protocol".to_string());
    }
    if let Some(passphrase) = passphrase.filter(|value| !value.is_empty()) {
        let plaintext = serde_json::to_vec(bundle)
            .map_err(|error| format!("sync bundle cannot be serialized: {error}"))?;
        let mut salt = [0_u8; SALT_LEN];
        let mut nonce = [0_u8; NONCE_LEN];
        let random = SystemRandom::new();
        random
            .fill(&mut salt)
            .map_err(|_| "sync salt generation failed".to_string())?;
        random
            .fill(&mut nonce)
            .map_err(|_| "sync nonce generation failed".to_string())?;
        let key = derive_key(passphrase, &salt, PBKDF2_ITERATIONS)?;
        let mut ciphertext = plaintext;
        LessSafeKey::new(
            UnboundKey::new(&aead::AES_256_GCM, &key)
                .map_err(|_| "invalid sync encryption key".to_string())?,
        )
        .seal_in_place_append_tag(
            Nonce::assume_unique_for_key(nonce),
            Aad::from(ENCRYPTION_PROTOCOL),
            &mut ciphertext,
        )
        .map_err(|_| "sync bundle encryption failed".to_string())?;
        serde_json::to_vec(&StoredSyncBundle::Encrypted {
            protocol: ENCRYPTION_PROTOCOL.to_string(),
            iterations: PBKDF2_ITERATIONS,
            salt_hex: hex(&salt),
            nonce_hex: hex(&nonce),
            ciphertext_hex: hex(&ciphertext),
        })
        .map_err(|error| format!("encrypted sync envelope cannot be serialized: {error}"))
    } else {
        serde_json::to_vec(&StoredSyncBundle::Plain {
            bundle: bundle.clone(),
        })
        .map_err(|error| format!("sync bundle cannot be serialized: {error}"))
    }
}

pub fn decode_sync_bundle(bytes: &[u8], passphrase: Option<&str>) -> Result<SyncBundle, String> {
    let stored: StoredSyncBundle = serde_json::from_slice(bytes)
        .map_err(|error| format!("sync bundle is not a valid envelope: {error}"))?;
    let bundle = match stored {
        StoredSyncBundle::Plain { bundle } => bundle,
        StoredSyncBundle::Encrypted {
            protocol,
            iterations,
            salt_hex,
            nonce_hex,
            ciphertext_hex,
        } => {
            if protocol != ENCRYPTION_PROTOCOL {
                return Err("unsupported encrypted sync protocol".to_string());
            }
            if iterations != PBKDF2_ITERATIONS {
                return Err("unsupported sync key-derivation parameters".to_string());
            }
            let passphrase = passphrase
                .filter(|value| !value.is_empty())
                .ok_or_else(|| "sync passphrase is required".to_string())?;
            let salt = unhex(&salt_hex)?;
            let nonce = unhex(&nonce_hex)?;
            let nonce: [u8; NONCE_LEN] = nonce
                .try_into()
                .map_err(|_| "invalid sync nonce length".to_string())?;
            let key = derive_key(passphrase, &salt, iterations)?;
            let mut plaintext = unhex(&ciphertext_hex)?;
            let opened = LessSafeKey::new(
                UnboundKey::new(&aead::AES_256_GCM, &key)
                    .map_err(|_| "invalid sync decryption key".to_string())?,
            )
            .open_in_place(
                Nonce::assume_unique_for_key(nonce),
                Aad::from(ENCRYPTION_PROTOCOL),
                &mut plaintext,
            )
            .map_err(|_| "sync bundle authentication failed".to_string())?;
            serde_json::from_slice(opened)
                .map_err(|error| format!("decrypted sync bundle is invalid: {error}"))?
        }
    };
    if bundle.protocol_version != SYNC_PROTOCOL_VERSION {
        return Err("unsupported sync bundle protocol".to_string());
    }
    merge_sync_events(bundle.events.clone(), [])?;
    Ok(bundle)
}

fn derive_key(passphrase: &str, salt: &[u8], iterations: u32) -> Result<[u8; 32], String> {
    let iterations = NonZeroU32::new(iterations)
        .ok_or_else(|| "sync KDF iterations must be positive".to_string())?;
    let mut key = [0_u8; 32];
    pbkdf2::derive(
        pbkdf2::PBKDF2_HMAC_SHA256,
        iterations,
        salt,
        passphrase.as_bytes(),
        &mut key,
    );
    Ok(key)
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex(digest(&SHA256, bytes).as_ref())
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(DIGITS[(byte >> 4) as usize] as char);
        output.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    output
}

fn unhex(value: &str) -> Result<Vec<u8>, String> {
    if value.len() % 2 != 0 {
        return Err("invalid hexadecimal sync data".to_string());
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let text = std::str::from_utf8(pair)
                .map_err(|_| "invalid hexadecimal sync data".to_string())?;
            u8::from_str_radix(text, 16).map_err(|_| "invalid hexadecimal sync data".to_string())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(device: &str, sequence: i64, occurred_at_ms: i64, value: i64) -> SyncEvent {
        build_sync_event(
            device,
            sequence,
            occurred_at_ms,
            "task",
            "task-1",
            "upsert",
            &format!(r#"{{"value":{value}}}"#),
        )
        .unwrap()
    }

    #[test]
    fn merge_is_a_validated_deterministic_set_union() {
        let a = event("device-a", 1, 10, 1);
        let b = event("device-b", 1, 10, 2);
        let merged = merge_sync_events(vec![b.clone(), a.clone()], vec![a.clone()]).unwrap();
        assert_eq!(merged, vec![a.clone(), b.clone()]);
        assert_eq!(
            latest_entity_events(&merged).get(&("task".into(), "task-1".into())),
            Some(&b)
        );

        let mut tampered = a;
        tampered.payload_json = r#"{"value":99}"#.into();
        assert!(merge_sync_events(vec![tampered], []).is_err());
    }

    #[test]
    fn encrypted_bundle_round_trips_and_rejects_the_wrong_secret() {
        let event = event("device-a", 1, 10, 1);
        let bundle = make_sync_bundle("device-a", 20, vec![event]).unwrap();
        let encoded = encode_sync_bundle(&bundle, Some("correct horse battery staple")).unwrap();
        assert!(!String::from_utf8_lossy(&encoded).contains("task-1"));
        assert_eq!(
            decode_sync_bundle(&encoded, Some("correct horse battery staple")).unwrap(),
            bundle
        );
        assert!(decode_sync_bundle(&encoded, Some("wrong")).is_err());

        let plain = encode_sync_bundle(&bundle, None).unwrap();
        assert_eq!(decode_sync_bundle(&plain, None).unwrap(), bundle);
    }
}
