//! WHATWG URLPattern support.
//!
//! Pattern compilation uses Rust's linear-time `regex` engine through the
//! `urlpattern` crate. This keeps regular-expression matching memory safe and
//! resistant to catastrophic backtracking.

use std::borrow::Cow;

pub use urlpattern::{
    Error as UrlPatternError, RegexSyntax, UrlPatternComponentResult, UrlPatternInit,
    UrlPatternMatchInput, UrlPatternOptions, UrlPatternResult,
};

use crate::Url;

/// A compiled WHATWG URLPattern.
#[derive(Debug)]
pub struct UrlPattern {
    inner: urlpattern::UrlPattern,
    ignore_case: bool,
    undefined_on_empty: UndefinedOnEmpty,
}

#[derive(Debug, Default)]
struct UndefinedOnEmpty {
    protocol: Vec<String>,
    username: Vec<String>,
    password: Vec<String>,
    hostname: Vec<String>,
    port: Vec<String>,
    pathname: Vec<String>,
    search: Vec<String>,
    hash: Vec<String>,
}

impl UrlPattern {
    /// Compiles a structured pattern.
    pub fn parse(
        init: UrlPatternInit,
        options: UrlPatternOptions,
    ) -> Result<Self, UrlPatternError> {
        if [
            &init.protocol,
            &init.username,
            &init.password,
            &init.hostname,
            &init.port,
            &init.pathname,
            &init.search,
            &init.hash,
        ]
        .into_iter()
        .flatten()
        .any(|component| contains_invalid_regexp_extension(component))
        {
            return Err(UrlPatternError::RegExp(()));
        }
        let ignore_case = options.ignore_case;
        let inner = urlpattern::UrlPattern::parse(init, options)?;
        let undefined_on_empty = UndefinedOnEmpty {
            protocol: undefined_on_empty_groups(
                inner.protocol(),
                urlpattern::parser::Options::default(),
            ),
            username: undefined_on_empty_groups(
                inner.username(),
                urlpattern::parser::Options::default(),
            ),
            password: undefined_on_empty_groups(
                inner.password(),
                urlpattern::parser::Options::default(),
            ),
            hostname: undefined_on_empty_groups(
                inner.hostname(),
                urlpattern::parser::Options::hostname(),
            ),
            port: undefined_on_empty_groups(inner.port(), urlpattern::parser::Options::default()),
            pathname: undefined_on_empty_groups(
                inner.pathname(),
                urlpattern::parser::Options::pathname(),
            ),
            search: undefined_on_empty_groups(
                inner.search(),
                urlpattern::parser::Options::default(),
            ),
            hash: undefined_on_empty_groups(inner.hash(), urlpattern::parser::Options::default()),
        };
        Ok(Self {
            inner,
            ignore_case,
            undefined_on_empty,
        })
    }

    /// Parses and compiles a URLPattern constructor string.
    ///
    /// A base URL is required when the constructor string has no protocol.
    pub fn new(
        pattern: &str,
        base_url: Option<&str>,
        options: UrlPatternOptions,
    ) -> Result<Self, UrlPatternError> {
        let init = urlpattern::quirks::process_construct_pattern_input(
            urlpattern::quirks::StringOrInit::String(Cow::Borrowed(pattern)),
            base_url,
        )?;
        Self::parse(init, options)
    }

    /// Tests a URL string, optionally resolving it against a base URL.
    pub fn test(&self, input: &str, base_url: Option<&str>) -> Result<bool, UrlPatternError> {
        let Some((input, _)) = urlpattern::quirks::process_match_input(
            urlpattern::quirks::StringOrInit::String(Cow::Borrowed(input)),
            base_url,
        )?
        else {
            return Ok(false);
        };
        self.inner.test(input)
    }

    /// Executes the pattern against a URL string.
    pub fn exec(
        &self,
        input: &str,
        base_url: Option<&str>,
    ) -> Result<Option<UrlPatternResult>, UrlPatternError> {
        let Some((input, _)) = urlpattern::quirks::process_match_input(
            urlpattern::quirks::StringOrInit::String(Cow::Borrowed(input)),
            base_url,
        )?
        else {
            return Ok(None);
        };
        self.exec_input(input)
    }

    /// Tests a structured URLPattern input.
    pub fn test_init(&self, init: UrlPatternInit) -> Result<bool, UrlPatternError> {
        self.test_input(UrlPatternMatchInput::Init(init))
    }

    /// Executes the pattern against a structured URLPattern input.
    pub fn exec_init(
        &self,
        init: UrlPatternInit,
    ) -> Result<Option<UrlPatternResult>, UrlPatternError> {
        self.exec_input(UrlPatternMatchInput::Init(init))
    }

    /// Tests a preprocessed URLPattern match input.
    pub fn test_input(&self, input: UrlPatternMatchInput) -> Result<bool, UrlPatternError> {
        self.inner.test(input)
    }

    /// Executes the pattern against a preprocessed URLPattern match input.
    pub fn exec_input(
        &self,
        input: UrlPatternMatchInput,
    ) -> Result<Option<UrlPatternResult>, UrlPatternError> {
        self.inner.exec(input).map(|result| {
            result.map(|mut result| {
                normalize_empty_groups(&mut result.protocol, &self.undefined_on_empty.protocol);
                normalize_empty_groups(&mut result.username, &self.undefined_on_empty.username);
                normalize_empty_groups(&mut result.password, &self.undefined_on_empty.password);
                normalize_empty_groups(&mut result.hostname, &self.undefined_on_empty.hostname);
                normalize_empty_groups(&mut result.port, &self.undefined_on_empty.port);
                normalize_empty_groups(&mut result.pathname, &self.undefined_on_empty.pathname);
                normalize_empty_groups(&mut result.search, &self.undefined_on_empty.search);
                normalize_empty_groups(&mut result.hash, &self.undefined_on_empty.hash);
                result
            })
        })
    }

    /// Tests an already parsed [`Url`] without reparsing its serialization.
    pub fn test_url(&self, url: &Url) -> Result<bool, UrlPatternError> {
        self.test_init(init_from_url(url))
    }

    /// Executes the pattern against an already parsed [`Url`].
    pub fn exec_url(&self, url: &Url) -> Result<Option<UrlPatternResult>, UrlPatternError> {
        self.exec_init(init_from_url(url))
    }

    /// Returns the protocol pattern.
    #[must_use]
    pub fn protocol(&self) -> &str {
        self.inner.protocol()
    }

    /// Returns the username pattern.
    #[must_use]
    pub fn username(&self) -> &str {
        self.inner.username()
    }

    /// Returns the password pattern.
    #[must_use]
    pub fn password(&self) -> &str {
        self.inner.password()
    }

    /// Returns the hostname pattern.
    #[must_use]
    pub fn hostname(&self) -> &str {
        self.inner.hostname()
    }

    /// Returns the port pattern.
    #[must_use]
    pub fn port(&self) -> &str {
        self.inner.port()
    }

    /// Returns the pathname pattern.
    #[must_use]
    pub fn pathname(&self) -> &str {
        self.inner.pathname()
    }

    /// Returns the query pattern without its leading `?`.
    #[must_use]
    pub fn search(&self) -> &str {
        self.inner.search()
    }

    /// Returns the fragment pattern without its leading `#`.
    #[must_use]
    pub fn hash(&self) -> &str {
        self.inner.hash()
    }

    /// Returns whether matching is ASCII case-insensitive where specified.
    #[must_use]
    pub const fn ignore_case(&self) -> bool {
        self.ignore_case
    }

    /// Returns whether any component contains an explicit regular expression.
    #[must_use]
    pub fn has_regexp_groups(&self) -> bool {
        self.inner.has_regexp_groups()
    }

    /// Tests all URL components at once.
    #[allow(clippy::too_many_arguments)]
    pub fn test_components(
        &self,
        protocol: &str,
        username: &str,
        password: &str,
        hostname: &str,
        port: &str,
        pathname: &str,
        search: &str,
        hash: &str,
    ) -> Result<bool, UrlPatternError> {
        self.test_init(UrlPatternInit {
            protocol: Some(protocol.to_owned()),
            username: Some(username.to_owned()),
            password: Some(password.to_owned()),
            hostname: Some(hostname.to_owned()),
            port: Some(port.to_owned()),
            pathname: Some(pathname.to_owned()),
            search: Some(search.to_owned()),
            hash: Some(hash.to_owned()),
            base_url: None,
        })
    }
}

fn init_from_url(url: &Url) -> UrlPatternInit {
    UrlPatternInit {
        protocol: Some(url.scheme().to_owned()),
        username: Some(url.username().to_owned()),
        password: Some(url.password().to_owned()),
        hostname: Some(url.hostname().to_owned()),
        port: Some(url.port().to_owned()),
        pathname: Some(url.pathname().to_owned()),
        search: Some(
            url.search()
                .strip_prefix('?')
                .unwrap_or_default()
                .to_owned(),
        ),
        hash: Some(url.hash().strip_prefix('#').unwrap_or_default().to_owned()),
        base_url: None,
    }
}

fn undefined_on_empty_groups(pattern: &str, options: urlpattern::parser::Options) -> Vec<String> {
    urlpattern::parser::parse_pattern_string(pattern, &options, |value| Ok(value.to_owned()))
        .map(|parts| {
            parts
                .into_iter()
                .filter(|part| {
                    part.modifier == urlpattern::parser::PartModifier::Optional
                        && part.prefix.is_empty()
                        && part.suffix.is_empty()
                })
                .map(|part| part.name)
                .collect()
        })
        .unwrap_or_default()
}

fn normalize_empty_groups(result: &mut UrlPatternComponentResult, names: &[String]) {
    for name in names {
        if result
            .groups
            .get(name)
            .is_some_and(|value| value.as_ref().is_some_and(std::string::String::is_empty))
        {
            result.groups.insert(name.clone(), None);
        }
    }
}

fn contains_invalid_regexp_extension(pattern: &str) -> bool {
    if pattern.contains("(?R)") {
        return true;
    }
    let bytes = pattern.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'\\' {
            index += 1;
            continue;
        }
        let start = index;
        while index < bytes.len() && bytes[index] == b'\\' {
            index += 1;
        }
        if (index - start) % 2 == 1 && bytes.get(index) == Some(&b'H') {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::{UrlPattern, UrlPatternInit, UrlPatternOptions};
    use crate::Url;

    #[test]
    fn constructor_string_and_captures() {
        let pattern = UrlPattern::new(
            "https://example.com/books/:id",
            None,
            UrlPatternOptions::default(),
        )
        .unwrap();
        let result = pattern
            .exec("https://example.com/books/42", None)
            .unwrap()
            .unwrap();
        assert_eq!(
            result.pathname.groups.get("id").and_then(Option::as_deref),
            Some("42")
        );
    }

    #[test]
    fn accepts_native_url_without_reparse() {
        let pattern = UrlPattern::parse(
            UrlPatternInit {
                hostname: Some("*.example.com".to_owned()),
                pathname: Some("/users/:id".to_owned()),
                ..UrlPatternInit::default()
            },
            UrlPatternOptions::default(),
        )
        .unwrap();
        let url = Url::parse("https://api.example.com/users/123", None).unwrap();
        assert!(pattern.test_url(&url).unwrap());
    }
}
