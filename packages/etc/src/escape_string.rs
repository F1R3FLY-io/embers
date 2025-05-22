use std::borrow::Cow;

use html_escape::encode_safe;

pub fn escape_string<'a>(input: &'a str) -> Cow<'a, str> {
    encode_safe(input)
}
