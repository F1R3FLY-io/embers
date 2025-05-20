use std::string::FromUtf8Error;

#[derive(Debug)]
pub struct Contract(Vec<u8>);

impl TryFrom<Contract> for String {
    type Error = FromUtf8Error;

    fn try_from(value: Contract) -> Result<Self, Self::Error> {
        String::from_utf8(value.0)
    }
}

impl From<String> for Contract {
    fn from(value: String) -> Self {
        Contract(value.into_bytes())
    }
}
