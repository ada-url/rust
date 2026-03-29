/// IDNA — pure-Rust implementation based on Ada's ada_idna.cpp.
/// IDNA utilities.
pub struct Idna {}

impl Idna {
    /// Convert a domain name to ASCII/Punycode (UTS #46).
    /// Returns an empty string on failure.
    ///
    /// ```
    /// use ada_url::Idna;
    /// assert_eq!(Idna::ascii("meßagefactory.ca"), "xn--meagefactory-m9a.ca");
    /// ```
    #[must_use]
    #[cfg(feature = "std")]
    pub fn ascii(input: &str) -> std::string::String {
        crate::idna_impl::domain_to_ascii(input).unwrap_or_default()
    }

    /// Convert an ACE/Punycode domain back to Unicode.
    ///
    /// This is a best-effort conversion: ACE labels (`xn--…`) that cannot be
    /// decoded as valid Punycode are kept as-is rather than returning an error.
    ///
    /// ```
    /// use ada_url::Idna;
    /// assert_eq!(Idna::unicode("xn--meagefactory-m9a.ca"), "meßagefactory.ca");
    /// ```
    #[must_use]
    #[cfg(feature = "std")]
    pub fn unicode(input: &str) -> std::string::String {
        crate::idna_impl::domain_to_unicode(input)
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "std")]
    use crate::Idna;

    #[test]
    #[cfg(feature = "std")]
    fn unicode_works() {
        assert_eq!(Idna::unicode("xn--meagefactory-m9a.ca"), "meßagefactory.ca");
    }

    #[test]
    #[cfg(feature = "std")]
    fn ascii_works() {
        assert_eq!(Idna::ascii("meßagefactory.ca"), "xn--meagefactory-m9a.ca");
    }
}
