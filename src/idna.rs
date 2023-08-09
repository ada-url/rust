use crate::ffi;

/// IDNA struct implements the to_ascii and to_unicode functions from the Unicode Technical
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
    pub fn unicode(input: &str) -> &str {
        unsafe {
            let out = ffi::ada_idna_to_unicode(input.as_ptr().cast(), input.len());
            let slice = std::slice::from_raw_parts(out.data.cast(), out.length);
            std::str::from_utf8_unchecked(slice)
        }
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
    pub fn ascii(input: &str) -> &str {
        unsafe {
            let out = ffi::ada_idna_to_ascii(input.as_ptr().cast(), input.len());
            let slice = std::slice::from_raw_parts(out.data.cast(), out.length);
            std::str::from_utf8_unchecked(slice)
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::idna::*;

    #[test]
    fn unicode_should_work() {
        assert_eq!(Idna::unicode("xn--meagefactory-m9a.ca"), "meßagefactory.ca");
    }

    #[test]
    fn ascii_should_work() {
        assert_eq!(Idna::ascii("meßagefactory.ca"), "xn--meagefactory-m9a.ca");
    }
}
