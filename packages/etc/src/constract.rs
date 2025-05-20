use std::{ops::Deref, string::FromUtf8Error};

#[derive(Debug, Clone)]
pub struct Code(Vec<u8>);

impl TryFrom<Code> for String {
    type Error = FromUtf8Error;

    fn try_from(value: Code) -> Result<Self, Self::Error> {
        String::from_utf8(value.0)
    }
}

impl From<Vec<u8>> for Code {
    fn from(value: Vec<u8>) -> Self {
        Self(value)
    }
}

impl From<String> for Code {
    fn from(value: String) -> Self {
        Code(value.into_bytes())
    }
}

impl Deref for Code {
    type Target = Vec<u8>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<Code> for Vec<u8> {
    fn from(value: Code) -> Self {
        value.0
    }
}
