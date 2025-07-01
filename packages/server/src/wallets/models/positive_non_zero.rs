#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PositiveNonZero<T>(pub T);

#[derive(Debug, Clone, thiserror::Error)]
pub enum PositiveNonZeroParsingError {
    #[error("value is zero")]
    Zero,
    #[error("value is negative")]
    Negative,
}

impl TryFrom<i64> for PositiveNonZero<i64> {
    type Error = PositiveNonZeroParsingError;

    fn try_from(value: i64) -> Result<Self, Self::Error> {
        if value == 0 {
            return Err(Self::Error::Zero);
        }

        if value < 0 {
            return Err(Self::Error::Negative);
        }

        Ok(Self(value))
    }
}
