#[derive(Debug, Clone)]
pub struct Code(String);

impl From<Code> for String {
    fn from(value: Code) -> String {
        value.0
    }
}

impl From<String> for Code {
    fn from(value: String) -> Self {
        Code(value)
    }
}
