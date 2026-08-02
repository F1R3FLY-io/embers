use chrono::Utc;
use derive_more::Into;
use serde::{Deserialize, de};

#[derive(Debug, Clone, Into)]
pub struct DateTime(chrono::DateTime<Utc>);

impl<'de> Deserialize<'de> for DateTime {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = i64::deserialize(deserializer)?;
        chrono::DateTime::from_timestamp_secs(value)
            .map(Self)
            .ok_or_else(|| de::Error::custom("invalid timestamp"))
    }
}

#[derive(Debug, Clone, Into)]
pub struct Uri(firefly_client::models::Uri);

impl<'de> Deserialize<'de> for Uri {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        firefly_client::models::Uri::try_from(value)
            .map(Self)
            .map_err(de::Error::custom)
    }
}

/// Reverse of `firefly_client::rendering::escape_rho_string`. The f1r3node
/// tuplespace stores Rholang string literals verbatim — escape sequences are
/// not processed by the parser (verified: `ch!("a\"b")` reads back as `a\"b`).
/// So user strings embers embedded into Rholang source via `escape_rho_string`
/// (`\` → `\\`, `"` → `\"`) come back still-escaped. Undo that one level, at the
/// point the user-facing value is materialised. Order is the reverse of the
/// escape (`\"` before `\\`), matching the `Graph` deserializer.
pub fn unescape_rho_string(s: &str) -> String {
    s.replace("\\\"", "\"").replace("\\\\", "\\")
}

/// serde helper: deserialize a required `String` and reverse the one Rholang
/// source-escaping layer added by `firefly_client::rendering::Value`.
pub fn deserialize_unescaped<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    String::deserialize(deserializer).map(|value| unescape_rho_string(&value))
}

/// serde helper: deserialize an `Option<String>` and reverse the Rholang string
/// escaping (see [`unescape_rho_string`]). Used for user-authored string fields
/// (`code`, `query`, …) that are read back out of the tuplespace for display or
/// re-deploy.
pub fn deserialize_unescaped_opt<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Option::<String>::deserialize(deserializer)?
        .as_deref()
        .map(unescape_rho_string))
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct RequiredValue {
        #[serde(deserialize_with = "super::deserialize_unescaped")]
        value: String,
    }

    #[test]
    fn required_rholang_string_is_unescaped_exactly_once() {
        let decoded: RequiredValue = serde_json::from_value(serde_json::json!({
            "value": "{\\\"schemaVersion\\\":\\\"2.0\\\",\\\"path\\\":\\\"C:\\\\workflow\\\"}"
        }))
        .unwrap();

        assert_eq!(
            decoded.value,
            r#"{"schemaVersion":"2.0","path":"C:\workflow"}"#
        );
    }
}

#[derive(Debug, Clone, Into)]
pub struct Hex(Vec<u8>);

impl<'de> Deserialize<'de> for Hex {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        hex::decode(value).map(Self).map_err(de::Error::custom)
    }
}
