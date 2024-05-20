#[cfg(feature = "std")]
extern crate std;

#[cfg_attr(not(feature = "std"), allow(unused_imports))]
use crate::ffi;

#[cfg(feature = "std")]
use std::string::String;

/// IDNA struct implements the `to_ascii` and `to_unicode` functions from the Unicode Technical
/// Standard supporting a wide range of systems. It is suitable for URL parsing.
/// For more information, [read the specification](https://www.unicode.org/reports/tr46/#ToUnicode)
pub struct Idna {}

impl Idna {
    /// Process international domains according to the UTS #46 standard.
    /// Returns empty string if the input is invalid.
    ///
    /// For more information, [read the specification](https://www.unicode.org/reports/tr46/#ToUnicode)
    ///
    /// ```
    /// use ada_url::Idna;
    /// assert_eq!(Idna::unicode("xn--meagefactory-m9a.ca"), "meßagefactory.ca");
    /// ```
    #[must_use]
    #[cfg(feature = "std")]
    pub fn unicode(input: &str) -> String {
        unsafe { ffi::ada_idna_to_unicode(input.as_ptr().cast(), input.len()) }.to_string()
    }

    /// Process international domains according to the UTS #46 standard.
    /// Returns empty string if the input is invalid.
    ///
    /// For more information, [read the specification](https://www.unicode.org/reports/tr46/#ToASCII)
    ///
    /// ```
    /// use ada_url::Idna;
    /// assert_eq!(Idna::ascii("meßagefactory.ca"), "xn--meagefactory-m9a.ca");
    /// ```
    #[must_use]
    #[cfg(feature = "std")]
    pub fn ascii(input: &str) -> String {
        unsafe { ffi::ada_idna_to_ascii(input.as_ptr().cast(), input.len()) }.to_string()
    }
}

#[cfg(test)]
mod tests {
    #[cfg_attr(not(feature = "std"), allow(unused_imports))]
    use crate::idna::*;

    #[test]
    fn unicode_should_work() {
        #[cfg(feature = "std")]
        assert_eq!(Idna::unicode("xn--meagefactory-m9a.ca"), "meßagefactory.ca");
    }

    #[test]
    fn ascii_should_work() {
        #[cfg(feature = "std")]
        assert_eq!(Idna::ascii("meßagefactory.ca"), "xn--meagefactory-m9a.ca");
    }
}
