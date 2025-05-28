use poem_openapi::Enum;

use crate::wallet::models::Direction;

#[derive(Debug, Clone, Enum)]
#[oai(rename_all = "lowercase")]
pub enum DirectionDto {
    Incoming,
    Outgoing,
}

impl From<Direction> for DirectionDto {
    fn from(value: Direction) -> Self {
        match value {
            Direction::Incoming => Self::Incoming,
            Direction::Outgoing => Self::Outgoing,
        }
    }
}
