use std::{collections::{BTreeMap, HashMap}, ops::Range};

use serde::{Serialize, Deserialize};
use serde_repr::{Serialize_repr, Deserialize_repr};


#[derive(Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct Game {
    pub name: String,
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
