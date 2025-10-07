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
