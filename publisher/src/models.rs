use chrono::{DateTime, Utc};
use serde::{Deserialize, de::Error as SerdeError, Deserializer, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize, Serialize)]
pub struct ServerData {
    #[serde(default, deserialize_with = "port_or_string")]
    pub port: Option<u16>,

    #[serde(default, deserialize_with = "int_or_string")]
    pub version: Option<String>,

    #[serde(rename = "electrum_version", default, deserialize_with = "int_or_string")]
    pub electrum_version: Option<String>,

    #[serde(rename = "LastUpdated", default, deserialize_with = "deserialize_datetime")]
    pub last_updated: Option<DateTime<Utc>>,

    #[serde(default)]
    pub user_submitted: bool,

    #[serde(default)]
    pub check_id: Option<String>,
}

// Custom deserializer functions
fn int_or_string<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let val = Value::deserialize(deserializer)?;
    Ok(match val {
        Value::String(s) => Some(s),
        Value::Number(n) => Some(n.to_string()),
        Value::Null => None,
        _ => return Err(SerdeError::custom("expected string or number")),
    })
}

fn port_or_string<'de, D>(deserializer: D) -> Result<Option<u16>, D::Error>
where
    D: Deserializer<'de>,
{
    let val = Value::deserialize(deserializer)?;
    match val {
        Value::Null => Ok(None),
        Value::Number(n) => {
            let num = n.as_u64().ok_or_else(|| SerdeError::custom("port must be a positive integer"))?;
            if num <= u16::MAX as u64 {
                Ok(Some(num as u16))
            } else {
                Err(SerdeError::custom("port out of range for u16"))
            }
        },
        Value::String(s) => {
            let parsed = s.parse::<u16>()
                .map_err(|_| SerdeError::custom("invalid string for port"))?;
            Ok(Some(parsed))
        },
        _ => Err(SerdeError::custom("expected a number or string for port")),
    }
}

fn deserialize_datetime<'de, D>(deserializer: D) -> Result<Option<DateTime<Utc>>, D::Error>
where
    D: Deserializer<'de>,
{
    let s: String = Deserialize::deserialize(deserializer)?;
    
    if s == "0001-01-01T00:00:00" {
        return Ok(None);
    }
    
    match DateTime::parse_from_rfc3339(&s) {
        Ok(dt) => Ok(Some(dt.with_timezone(&Utc))),
        Err(_) => Ok(None),
    }
} 