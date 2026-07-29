//! WHATWG percent-encoding helpers.

use alloc::string::{String, ToString};
use percent_encoding::{AsciiSet, CONTROLS, percent_decode_str, utf8_percent_encode};

const FRAGMENT: &AsciiSet = &CONTROLS.add(b' ').add(b'"').add(b'<').add(b'>').add(b'`');
const QUERY: &AsciiSet = &CONTROLS.add(b' ').add(b'"').add(b'#').add(b'<').add(b'>');
const SPECIAL_QUERY: &AsciiSet = &QUERY.add(b'\'');
const PATH: &AsciiSet = &QUERY.add(b'?').add(b'^').add(b'`').add(b'{').add(b'}');
const USERINFO: &AsciiSet = &PATH
    .add(b'/')
    .add(b':')
    .add(b';')
    .add(b'=')
    .add(b'@')
    .add(b'[')
    .add(b'\\')
    .add(b']')
    .add(b'|');
const COMPONENT: &AsciiSet = &USERINFO.add(b'$').add(b'%').add(b'&').add(b'+').add(b',');
const FORM_URLENCODED: &AsciiSet = &COMPONENT.add(b'!').add(b'\'').add(b'(').add(b')').add(b'~');

/// A WHATWG percent-encode set.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum PercentEncodeSet {
    /// C0 controls and all non-ASCII bytes.
    C0Control,
    /// The fragment percent-encode set.
    Fragment,
    /// The query percent-encode set.
    Query,
    /// The special-query percent-encode set.
    SpecialQuery,
    /// The path percent-encode set.
    Path,
    /// The userinfo percent-encode set.
    UserInfo,
    /// The component percent-encode set.
    Component,
    /// The `application/x-www-form-urlencoded` percent-encode set.
    FormUrlencoded,
}

impl PercentEncodeSet {
    fn ascii_set(self) -> &'static AsciiSet {
        match self {
            Self::C0Control => CONTROLS,
            Self::Fragment => FRAGMENT,
            Self::Query => QUERY,
            Self::SpecialQuery => SPECIAL_QUERY,
            Self::Path => PATH,
            Self::UserInfo => USERINFO,
            Self::Component => COMPONENT,
            Self::FormUrlencoded => FORM_URLENCODED,
        }
    }
}

/// Percent-encodes UTF-8 using a WHATWG encode set.
#[must_use]
pub fn percent_encode(input: &str, set: PercentEncodeSet) -> String {
    utf8_percent_encode(input, set.ascii_set()).to_string()
}

/// Percent-decodes a UTF-8 string, replacing malformed UTF-8 with U+FFFD.
#[must_use]
pub fn percent_decode(input: &str) -> String {
    percent_decode_str(input).decode_utf8_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::{PercentEncodeSet, percent_decode, percent_encode};

    #[test]
    fn encodes_named_sets() {
        assert_eq!(
            percent_encode("a b/☃", PercentEncodeSet::Path),
            "a%20b/%E2%98%83"
        );
        assert_eq!(
            percent_encode("a b+c", PercentEncodeSet::FormUrlencoded),
            "a%20b%2Bc"
        );
        assert_eq!(percent_decode("caf%C3%A9"), "café");
    }
}
