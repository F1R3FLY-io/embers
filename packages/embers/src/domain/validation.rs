use std::collections::BTreeMap;

/// Safe, actionable validation failures. Never include credentials or upstream
/// response bodies: these errors cross the public API boundary.
#[derive(Debug, Clone, thiserror::Error)]
#[error("{description}")]
pub struct ValidationError {
    pub code: String,
    pub description: String,
    pub fields: BTreeMap<String, Vec<String>>,
}

impl ValidationError {
    pub fn field(field: &str, code: &str, message: impl Into<String>) -> Self {
        let description = message.into();
        Self {
            code: code.into(),
            fields: BTreeMap::from([(field.into(), vec![description.clone()])]),
            description,
        }
    }
}
