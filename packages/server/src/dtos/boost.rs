use firefly_client::models::Boost;
use poem_openapi::Object;

use super::direction::DirectionDto;

#[derive(Debug, Clone, Object)]
pub struct BoostDto {
    pub id: String,
    pub username: String,
    pub direction: DirectionDto,
    pub date: String,
    pub amount: String,
    pub post: String,
}

impl From<Boost> for BoostDto {
    fn from(value: Boost) -> Self {
        Self {
            id: value.id,
            username: value.username,
            direction: value.direction.into(),
            date: value.date,
            amount: value.amount,
            post: value.post,
        }
    }
}
