use derive_more::{AsRef, Into};
use thiserror::Error;

#[derive(Debug, Clone, Default, Into, AsRef)]
pub struct Description(String);

const MAX_DESCRIPTION_CHARS_COUNT: usize = 512;

#[derive(Debug, Clone, Error)]
pub enum DescriptionError {
    #[error("Maximum description length reached")]
    TooLong,
}

impl TryFrom<String> for Description {
    type Error = DescriptionError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.chars().count() > MAX_DESCRIPTION_CHARS_COUNT {
            return Err(Self::Error::TooLong);
        }

        Ok(Self(html_escape::encode_safe(&value).into_owned()))
    }
}
