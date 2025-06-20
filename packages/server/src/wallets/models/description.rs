use derive_more::{AsRef, Into};
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Into, AsRef)]
pub struct Description(String);

const MAX_DESCRIPTION_CHARS_COUNT: usize = 512;

#[derive(Debug, Clone, Error)]
pub enum DescriptionError {
    #[error("maximum description length reached")]
    TooLong,
}

impl TryFrom<String> for Description {
    type Error = DescriptionError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.chars().count() > MAX_DESCRIPTION_CHARS_COUNT {
            return Err(Self::Error::TooLong);
        }

        Ok(Self(value))
    }
}
