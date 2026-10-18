//! Local, read-only reviewer export. Never pass database internals to this module:
//! the application adapter must supply only explicitly exportable research records.
use aes_gcm::{
    aead::{rand_core::RngCore, Aead, KeyInit, OsRng},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use pbkdf2::pbkdf2_hmac;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeSet, HashSet};

pub const ITERATIONS: u32 = 600_000;
const AAD: &[u8] = b"CodingStudio reviewer export v1";
const MAX_JSON_BYTES: usize = 256 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Record {
    /// Non-identifying, stable identifiers, e.g. ["interviews", "ECO-41"].
    pub path: Vec<String>,
    pub value: Value,
    pub selected_by_default: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Field {
    pub id: String,
    pub path: Vec<String>,
    pub value: Value,
    pub selected_by_default: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Catalogue {
    pub snapshot: String,
    pub fields: Vec<Field>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SelectedField {
    pub path: Vec<String>,
    pub value: Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Payload {
    pub format: String,
    pub fields: Vec<SelectedField>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub export_info: Option<Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Envelope {
    pub format: String,
    pub iterations: u32,
    pub salt: String,
    pub iv: String,
    pub ciphertext: String,
}

pub struct CryptoSession {
    cipher: Aes256Gcm,
    salt: [u8; 16],
}

impl CryptoSession {
    pub fn new(password: &str) -> Result<Self, String> {
        if password.chars().count() < 16 {
            return Err("Use a password of at least 16 characters".into());
        }
        let mut salt = [0u8; 16];
        OsRng.fill_bytes(&mut salt);
        let mut key = [0u8; 32];
        pbkdf2_hmac::<Sha256>(password.as_bytes(), &salt, ITERATIONS, &mut key);
        let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| "Cannot initialise encryption")?;
        key.fill(0);
        Ok(Self { cipher, salt })
    }

    /// Every part uses a fresh IV and authenticated identity/order.
    pub fn chunk(&self, bytes: &[u8], identity: &str) -> Result<(Vec<u8>, String), String> {
        let mut iv = [0u8; 12];
        OsRng.fill_bytes(&mut iv);
        let ciphertext = self
            .cipher
            .encrypt(
                Nonce::from_slice(&iv),
                aes_gcm::aead::Payload {
                    msg: bytes,
                    aad: identity.as_bytes(),
                },
            )
            .map_err(|_| "Cannot encrypt source chunk")?;
        Ok((ciphertext, STANDARD.encode(iv)))
    }

    pub fn payload(&self, payload: &Payload) -> Result<Envelope, String> {
        let bytes = serde_json::to_vec(payload).map_err(|_| "Cannot serialize selected content")?;
        if bytes.len() > MAX_JSON_BYTES {
            return Err("Selected text exceeds the 256 MiB limit".into());
        }
        let (ciphertext, iv) =
            self.chunk(&bytes, std::str::from_utf8(AAD).expect("constant ASCII"))?;
        Ok(Envelope {
            format: "coding-studio-reviewer-aes256gcm-v1".into(),
            iterations: ITERATIONS,
            salt: STANDARD.encode(self.salt),
            iv,
            ciphertext: STANDARD.encode(ciphertext),
        })
    }
}

fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn flatten(path: &mut Vec<String>, value: &Value, default: bool, fields: &mut Vec<Field>) {
    match value {
        Value::Object(items) if !items.is_empty() => {
            // Sort even when serde_json is compiled with preserve_order elsewhere.
            let mut keys: Vec<_> = items.keys().collect();
            keys.sort();
            for key in keys {
                path.push(key.clone());
                flatten(path, &items[key], default, fields);
                path.pop();
            }
        }
        Value::Array(items) if !items.is_empty() => {
            for (i, item) in items.iter().enumerate() {
                path.push(i.to_string());
                flatten(path, item, default, fields);
                path.pop();
            }
        }
        _ => fields.push(Field {
            id: digest(&serde_json::to_vec(path).expect("string paths serialize")),
            path: path.clone(),
            value: value.clone(),
            selected_by_default: default,
        }),
    }
}

pub fn catalogue(records: &[Record]) -> Result<Catalogue, String> {
    let mut fields = Vec::new();
    for record in records {
        if record.path.is_empty() || record.path[0].is_empty() {
            return Err("An export record has no stable path".into());
        }
        flatten(
            &mut record.path.clone(),
            &record.value,
            record.selected_by_default,
            &mut fields,
        );
    }
    fields.sort_by(|a, b| a.path.cmp(&b.path));
    let unique: HashSet<_> = fields.iter().map(|f| &f.id).collect();
    if unique.len() != fields.len() {
        return Err("Duplicate export field path".into());
    }
    let bytes = serde_json::to_vec(&fields).map_err(|_| "Cannot serialize export fields")?;
    if bytes.len() > MAX_JSON_BYTES {
        return Err("Text export exceeds the 256 MiB limit; select fewer records".into());
    }
    Ok(Catalogue {
        snapshot: digest(&bytes),
        fields,
    })
}

pub fn select(
    catalogue: &Catalogue,
    snapshot: &str,
    selected: &[String],
) -> Result<Payload, String> {
    if snapshot != catalogue.snapshot {
        return Err("The project has changed. Refresh the export selection".into());
    }
    if selected.is_empty() {
        return Err("Select at least one field".into());
    }
    let wanted: BTreeSet<_> = selected.iter().collect();
    if wanted.len() != selected.len() {
        return Err("Duplicate selected field".into());
    }
    let known: HashSet<_> = catalogue.fields.iter().map(|f| &f.id).collect();
    if wanted.iter().any(|id| !known.contains(*id)) {
        return Err("Unknown selected field".into());
    }
    let fields = catalogue
        .fields
        .iter()
        .filter(|f| wanted.contains(&f.id))
        .map(|f| SelectedField {
            path: f.path.clone(),
            value: f.value.clone(),
        })
        .collect();
    Ok(Payload {
        format: "coding-studio-reviewer-data-v1".into(),
        fields,
        export_info: None,
    })
}

pub fn encrypt(payload: &Payload, password: &str) -> Result<Envelope, String> {
    CryptoSession::new(password)?.payload(payload)
}

/// One portable HTML file, with no CDN, telemetry or clear-text research data. Attachments load on demand from the same origin.
pub fn reader_html(envelope: &Envelope) -> Result<String, String> {
    let data = STANDARD
        .encode(serde_json::to_vec(envelope).map_err(|_| "Cannot serialize encrypted export")?);
    let script = format!(
        "{}\n{}",
        include_str!("../web/reader-model.js"),
        include_str!("../web/reader.js")
    );
    let licence = include_str!("../../../LICENSE")
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    let hash = STANDARD.encode(Sha256::digest(script.as_bytes()));
    Ok(include_str!("../web/reader.html")
        .replace("__SCRIPT_HASH__", &hash)
        .replace("__ENCRYPTED_PAYLOAD__", &data)
        .replace("__LICENSE_TEXT__", &licence)
        .replace("__READER_SCRIPT__", &script))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fixture() -> Catalogue {
        catalogue(&[Record { path: vec!["interviews".into(), "I01".into()],
            value: json!({"units": [{"text":"SELECTED QUOTE", "speaker":"EXCLUDED NAME"}], "memo":"EXCLUDED NOTE"}),
            selected_by_default:true }]).unwrap()
    }
    fn decrypt(e: &Envelope, password: &str) -> Result<Vec<u8>, aes_gcm::Error> {
        let salt = STANDARD.decode(&e.salt).unwrap();
        let iv = STANDARD.decode(&e.iv).unwrap();
        let bytes = STANDARD.decode(&e.ciphertext).unwrap();
        let mut key = [0u8; 32];
        pbkdf2_hmac::<Sha256>(password.as_bytes(), &salt, e.iterations, &mut key);
        Aes256Gcm::new_from_slice(&key).unwrap().decrypt(
            Nonce::from_slice(&iv),
            aes_gcm::aead::Payload {
                msg: &bytes,
                aad: AAD,
            },
        )
    }
    #[test]
    fn field_selection_is_exact_and_no_excluded_content_leaks() {
        let c = fixture();
        let id = c
            .fields
            .iter()
            .find(|f| f.path.last().unwrap() == "text")
            .unwrap()
            .id
            .clone();
        let p = select(&c, &c.snapshot, &[id]).unwrap();
        assert_eq!(p.fields.len(), 1);
        let encoded = serde_json::to_string(&p).unwrap();
        assert!(encoded.contains("SELECTED QUOTE"));
        assert!(!encoded.contains("EXCLUDED"));
        let e = encrypt(&p, "test password only 12345").unwrap();
        let html = reader_html(&e).unwrap();
        assert!(!html.contains("SELECTED QUOTE"));
        assert!(!html.contains("EXCLUDED"));
        assert!(!html.contains("test password only 12345"));
        assert_eq!(
            serde_json::from_slice::<Payload>(&decrypt(&e, "test password only 12345").unwrap())
                .unwrap()
                .fields,
            p.fields
        );
    }
    #[test]
    fn stale_unknown_empty_and_duplicate_selection_are_rejected() {
        let c = fixture();
        let id = c.fields[0].id.clone();
        assert!(select(&c, "stale", std::slice::from_ref(&id)).is_err());
        assert!(select(&c, &c.snapshot, &["unknown".into()]).is_err());
        assert!(select(&c, &c.snapshot, &[]).is_err());
        assert!(select(&c, &c.snapshot, &[id.clone(), id]).is_err());
    }
    #[test]
    fn wrong_password_and_modified_ciphertext_fail() {
        let c = fixture();
        let p = select(&c, &c.snapshot, &[c.fields[0].id.clone()]).unwrap();
        let mut e = encrypt(&p, "test password only 12345").unwrap();
        assert!(decrypt(&e, "different password 12345").is_err());
        let mut bytes = STANDARD.decode(&e.ciphertext).unwrap();
        bytes[0] ^= 1;
        e.ciphertext = STANDARD.encode(bytes);
        assert!(decrypt(&e, "test password only 12345").is_err());
    }
    #[test]
    fn every_export_gets_fresh_salt_and_nonce() {
        let c = fixture();
        let p = select(&c, &c.snapshot, &[c.fields[0].id.clone()]).unwrap();
        let a = encrypt(&p, "test password only 12345").unwrap();
        let b = encrypt(&p, "test password only 12345").unwrap();
        assert_ne!(a.salt, b.salt);
        assert_ne!(a.iv, b.iv);
        assert_ne!(a.ciphertext, b.ciphertext);
    }
    #[test]
    fn snapshot_changes_with_text_or_defaults_but_not_record_order() {
        let a = Record {
            path: vec!["a".into()],
            value: json!({"text":"a"}),
            selected_by_default: true,
        };
        let b = Record {
            path: vec!["b".into()],
            value: json!({"text":"b"}),
            selected_by_default: true,
        };
        assert_eq!(
            catalogue(&[a.clone(), b.clone()]).unwrap().snapshot,
            catalogue(&[b, a.clone()]).unwrap().snapshot
        );
        let mut changed = a.clone();
        changed.value = json!({"text":"new"});
        assert_ne!(
            catalogue(&[a]).unwrap().snapshot,
            catalogue(&[changed.clone()]).unwrap().snapshot
        );
        assert!(catalogue(&[changed.clone(), changed]).is_err());
    }
}
