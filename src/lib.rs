use std::{collections::{BTreeMap, HashMap}, ops::Range, env};

use aes_gcm::{KeyInit, Aes256Gcm, AeadCore, aead::{OsRng, Aead, generic_array::sequence::Lengthen, AeadMut}, Nonce};
use base64::Engine;
use hmac::Hmac;
use jwt::{SignWithKey, VerifyWithKey};
use lazy_static::lazy_static;
use serde::{Serialize, Deserialize};
use serde_repr::{Serialize_repr, Deserialize_repr};
use sha2::Sha256;
use thiserror::Error;


#[derive(Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct Game {
    pub name: String,
    pub main_page: Option<String>,
    pub suffix: String,
    pub uid: UidConfig,
    pub username: UsernameConfig,
    pub keys: BTreeMap<String, Key>
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct UidConfig {
    pub max_length: usize
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct UsernameConfig {
    pub optional: bool,
    pub max_length: usize
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct Key {
    #[serde(flatten)]
    pub ty: KeyType,
    pub name: String,
    #[serde(default)]
    pub name_localizations: Option<HashMap<String, String>>,
    pub description: String,
    #[serde(default)]
    pub description_localizations: Option<HashMap<String, String>>
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(tag = "type")]
pub enum KeyType {
    BoolEq { conditions: Vec<KeyCondition> }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum KeyCondition {
    Uid(Range<u64>)
}

#[derive(Serialize)]
pub struct PutRoleConnectionInfo<'a> {
    platform_name: &'a str,
    platform_username: String,
    metadata: HashMap<&'a str, String>
}

impl Game {
    pub fn make_role_connection_records(&self) -> Vec<RoleConnectionMetadataRecord> {
        self.keys.iter().map(|(key, value)| RoleConnectionMetadataRecord {
            ty: match &value.ty {
                KeyType::BoolEq { .. } => RoleConnectionMetadataRecordType::BoolEq
            },
            key: key.to_string(),
            name: value.name.clone(),
            name_localizations: value.name_localizations.clone(),
            description: value.description.clone(),
            description_localizations: value.description_localizations.clone()
        }).collect()
    }
    
    pub fn make_role_connection_info<'a>(&'a self, uid: u64, username: &'a str) -> PutRoleConnectionInfo<'a> {
        PutRoleConnectionInfo {
            platform_name: &self.name,
            platform_username: if username.is_empty() { uid.to_string() } else { format!("{} ({})", username, uid) },
            metadata: HashMap::from_iter(self.keys.iter()
                .map(|(k, v)| (k.as_str(), match &v.ty {
                    KeyType::BoolEq { conditions } => if conditions.iter().all(|c| match c {
                        KeyCondition::Uid(range) => range.contains(&uid),
                    }) { "1" } else { "0" }.to_string(),
                })))
        }
    }
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug)]
pub struct RoleConnectionMetadataRecord {
    #[serde(rename = "type")]
    pub ty: RoleConnectionMetadataRecordType,
    pub key: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name_localizations: Option<HashMap<String, String>>,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description_localizations: Option<HashMap<String, String>>
}

impl PartialOrd for RoleConnectionMetadataRecord {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.key.cmp(&other.key))
    }
}

impl Ord for RoleConnectionMetadataRecord {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.key.cmp(&other.key)
    }
}

#[derive(Serialize_repr, Deserialize_repr, PartialEq, Eq, Debug)]
#[repr(usize)]
#[non_exhaustive]
pub enum RoleConnectionMetadataRecordType {
    IntegerLtEq = 1,
    IntegerGtEq = 2,
    IntegerEq = 3,
    IntegerNotEq = 4,
    DatetimeLtEq = 5,
    DatetimeGtEq = 6,
    BoolEq = 7,
    BoolNotEq = 8
}

lazy_static! {
    static ref TOKEN_JWT_KEY: Hmac<Sha256> = Hmac::new_from_slice(&hex::decode(env::var("TOKEN_JWT_KEY").expect("No token JWT key!")).expect("Token JWT key must be hex!")).unwrap();
    static ref TOKEN_CIPHER_KEY_BYTES: [u8; 32] = hex::decode(env::var("TOKEN_CIPHER_KEY").expect("No token cipher key!")).expect("Token cipher key must be hex!").try_into().unwrap();
    static ref TOKEN_CIPHER_KEY: &'static aes_gcm::Key<Aes256Gcm> = TOKEN_CIPHER_KEY_BYTES.as_ref().into();
}

pub fn generate_encrypted_key(token: &str) -> String {
    let cipher = Aes256Gcm::new(&TOKEN_CIPHER_KEY);
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let mut ciphertext = cipher.encrypt(&nonce, token.as_bytes()).unwrap();

    let mut result = nonce.to_vec();
    result.append(&mut ciphertext);
    let claims = base64::engine::general_purpose::STANDARD_NO_PAD.encode(result);
    claims.sign_with_key(&*TOKEN_JWT_KEY).unwrap()
}

#[derive(Error, Debug)]
#[error("invalid token")]
pub struct InvalidToken;

pub fn decrypt_key(key: &str) -> Result<String, InvalidToken> {
    let cipher = Aes256Gcm::new(&TOKEN_CIPHER_KEY);
    let result: String = key.verify_with_key(&*TOKEN_JWT_KEY).map_err(|_| InvalidToken)?;
    let decoded = base64::engine::general_purpose::STANDARD_NO_PAD.decode(result).map_err(|_| InvalidToken)?;
    let nonce = Nonce::from_slice(&decoded[..12]);
    let ciphertext = &decoded[12..];

    let plaintext = cipher.decrypt(nonce, ciphertext).map_err(|_| InvalidToken)?;
    Ok(String::from_utf8(plaintext).unwrap())
}

#[cfg(test)]
mod tests {
    use crate::{generate_encrypted_key, decrypt_key};

    #[test]
    fn test_encryption() {
        let token = hex::encode("hello world this is a test token lmao oo it's long even longer than a real token oh boy");
        let encoded = generate_encrypted_key(&token);
        let decoded = decrypt_key(&encoded).unwrap();

        assert_eq!(token, decoded)
    }
}
