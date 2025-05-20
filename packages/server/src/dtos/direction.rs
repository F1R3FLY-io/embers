use firefly_client::models::Direction;
use poem_openapi::Enum;

#[derive(Debug, Clone, Enum)]
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
