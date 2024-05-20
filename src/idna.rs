use crate::ffi;

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
    /// assert_eq!(Idna::unicode("xn--meagefactory-m9a.ca").as_ref(), "meßagefactory.ca");
    /// ```
    #[must_use]
    pub fn unicode(input: &str) -> ffi::ada_owned_string {
        unsafe { ffi::ada_idna_to_unicode(input.as_ptr().cast(), input.len()) }
    }

    /// Process international domains according to the UTS #46 standard.
    /// Returns empty string if the input is invalid.
    ///
    /// For more information, [read the specification](https://www.unicode.org/reports/tr46/#ToASCII)
    ///
    /// ```
    /// use ada_url::Idna;
    /// assert_eq!(Idna::ascii("meßagefactory.ca").as_ref(), "xn--meagefactory-m9a.ca");
    /// ```
    #[must_use]
    pub fn ascii(input: &str) -> ffi::ada_owned_string {
        unsafe { ffi::ada_idna_to_ascii(input.as_ptr().cast(), input.len()) }
    }
}

#[cfg(test)]
mod tests {
    use crate::idna::*;

    #[test]
    fn unicode_should_work() {
        assert_eq!(
            Idna::unicode("xn--meagefactory-m9a.ca").as_ref(),
            "meßagefactory.ca"
        );
    }

    #[test]
    fn ascii_should_work() {
        assert_eq!(
            Idna::ascii("meßagefactory.ca").as_ref(),
            "xn--meagefactory-m9a.ca"
        );
    }
}
