//! URLPattern WPT integration.
//!
//! The fixture adapter follows the MIT-licensed `denoland/rust-urlpattern`
//! test harness, while exercising ada-url' public wrapper.

#![cfg(feature = "url-pattern")]

use std::collections::HashMap;

use ada_url::{
    UrlPattern, UrlPatternComponentResult, UrlPatternInit, UrlPatternOptions, UrlPatternResult,
};
use serde::{Deserialize, Serialize};
use urlpattern::quirks::{self, StringOrInit};

#[derive(Debug, Deserialize)]
#[serde(untagged)]
#[serde(bound(deserialize = "'de: 'a"))]
#[allow(clippy::large_enum_variant)]
enum ExpectedMatch<'a> {
    String(String),
    MatchResult(MatchResult<'a>),
}

#[derive(Debug, Deserialize)]
struct ComponentResult {
    input: String,
    groups: HashMap<String, Option<String>>,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
enum StringOrInitOrOptions<'a> {
    Options(UrlPatternOptions),
    StringOrInit(StringOrInit<'a>),
}

#[derive(Debug, Deserialize)]
#[serde(bound(deserialize = "'de: 'a"))]
struct TestCase<'a> {
    skip: Option<String>,
    pattern: Vec<StringOrInitOrOptions<'a>>,
    #[serde(default)]
    inputs: Vec<StringOrInit<'a>>,
    expected_obj: Option<StringOrInit<'a>>,
    expected_match: Option<ExpectedMatch<'a>>,
    #[serde(default)]
    exactly_empty_components: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(bound(deserialize = "'de: 'a"))]
struct MatchResult<'a> {
    #[serde(deserialize_with = "deserialize_match_result_inputs")]
    #[serde(default)]
    inputs: Option<(StringOrInit<'a>, Option<String>)>,
    protocol: Option<ComponentResult>,
    username: Option<ComponentResult>,
    password: Option<ComponentResult>,
    hostname: Option<ComponentResult>,
    port: Option<ComponentResult>,
    pathname: Option<ComponentResult>,
    search: Option<ComponentResult>,
    hash: Option<ComponentResult>,
}

fn deserialize_match_result_inputs<'a, D>(
    deserializer: D,
) -> Result<Option<(StringOrInit<'a>, Option<String>)>, D::Error>
where
    D: serde::Deserializer<'a>,
{
    #[derive(Debug, Deserialize)]
    #[serde(untagged)]
    enum Inputs<'a> {
        One((StringOrInit<'a>,)),
        Two(StringOrInit<'a>, String),
    }

    Ok(match Option::<Inputs>::deserialize(deserializer)? {
        Some(Inputs::One((input,))) => Some((input, None)),
        Some(Inputs::Two(input, base)) => Some((input, Some(base))),
        None => None,
    })
}

fn run_case(index: usize, case: TestCase<'_>) {
    if case.skip.is_some() {
        return;
    }

    let mut pattern_input = StringOrInit::Init(Default::default());
    let mut pattern_base = None;
    let mut options = None;
    for (argument, value) in case.pattern.into_iter().enumerate() {
        match value {
            StringOrInitOrOptions::StringOrInit(value) if argument == 0 => {
                pattern_input = value;
            }
            StringOrInitOrOptions::StringOrInit(StringOrInit::String(value)) if argument == 1 => {
                pattern_base = Some(value);
            }
            StringOrInitOrOptions::Options(value) => options = Some(value),
            StringOrInitOrOptions::StringOrInit(_)
                if matches!(
                    &case.expected_obj,
                    Some(StringOrInit::String(value)) if value == "error"
                ) =>
            {
                return;
            }
            StringOrInitOrOptions::StringOrInit(_) => {
                panic!("URLPattern case {index}: invalid constructor arguments");
            }
        }
    }

    let compiled =
        quirks::process_construct_pattern_input(pattern_input.clone(), pattern_base.as_deref())
            .and_then(|init| UrlPattern::parse(init, options.unwrap_or_default()));

    let expected_object = match case.expected_obj {
        Some(StringOrInit::String(value)) if value == "error" => {
            assert!(
                compiled.is_err(),
                "URLPattern case {index}: expected construction error"
            );
            return;
        }
        Some(StringOrInit::String(_)) => {
            panic!("URLPattern case {index}: unexpected string expected_obj")
        }
        Some(StringOrInit::Init(init)) => UrlPatternInit {
            protocol: init.protocol,
            username: init.username,
            password: init.password,
            hostname: init.hostname,
            port: init.port,
            pathname: init.pathname,
            search: init.search,
            hash: init.hash,
            base_url: init
                .base_url
                .map(|base| base.parse().expect("valid expected base URL")),
        },
        None => UrlPatternInit::default(),
    };
    let pattern =
        compiled.unwrap_or_else(|error| panic!("URLPattern case {index}: compile failed: {error}"));

    if let StringOrInit::Init(quirks::UrlPatternInit {
        base_url: Some(base),
        ..
    }) = &pattern_input
    {
        pattern_base = Some(base.clone().into());
    }

    macro_rules! assert_pattern {
        ($field:ident) => {{
            let mut expected = expected_object.$field;
            if expected.is_none() {
                if case
                    .exactly_empty_components
                    .iter()
                    .any(|component| component == stringify!($field))
                {
                    expected = Some(String::new());
                } else if let StringOrInit::Init(quirks::UrlPatternInit {
                    $field: Some(value),
                    ..
                }) = &pattern_input
                {
                    expected = Some(value.to_owned());
                } else if init_implies_wildcard(&pattern_input, stringify!($field)) {
                    expected = Some("*".to_owned());
                } else if let Some(base) = pattern_base
                    .as_ref()
                    .filter(|_| !matches!(stringify!($field), "username" | "password"))
                {
                    let base = url::Url::parse(base).expect("valid fixture base URL");
                    let value = match stringify!($field) {
                        "protocol" => base.scheme(),
                        "username" => base.username(),
                        "password" => base.password().unwrap_or_default(),
                        "hostname" => base.host_str().unwrap_or_default(),
                        "port" => url::quirks::port(&base),
                        "pathname" => url::quirks::pathname(&base),
                        "search" => base.query().unwrap_or_default(),
                        "hash" => base.fragment().unwrap_or_default(),
                        _ => unreachable!(),
                    };
                    expected = Some(value.to_owned());
                } else {
                    expected = Some("*".to_owned());
                }
            }
            assert_eq!(
                expected.as_deref().unwrap(),
                pattern.$field(),
                "URLPattern case {}: {} pattern",
                index,
                stringify!($field)
            );
        }};
    }

    assert_pattern!(protocol);
    assert_pattern!(username);
    assert_pattern!(password);
    assert_pattern!(hostname);
    assert_pattern!(port);
    assert_pattern!(pathname);
    assert_pattern!(search);
    assert_pattern!(hash);

    let input = case
        .inputs
        .first()
        .cloned()
        .unwrap_or_else(|| StringOrInit::Init(Default::default()));
    let base = case.inputs.get(1).map(|value| match value {
        StringOrInit::String(value) => value.clone(),
        StringOrInit::Init(_) => panic!("URLPattern case {index}: structured match base"),
    });
    let expected_inputs = (input.clone(), base.clone().map(String::from));
    let processed = quirks::process_match_input(input, base.as_deref());

    if matches!(
        &case.expected_match,
        Some(ExpectedMatch::String(value)) if value == "error"
    ) {
        assert!(
            processed.is_err(),
            "URLPattern case {index}: expected match-input error"
        );
        return;
    }

    let Some((match_input, actual_inputs)) = processed
        .unwrap_or_else(|error| panic!("URLPattern case {index}: match input failed: {error}"))
    else {
        assert!(
            case.expected_match.is_none(),
            "URLPattern case {index}: invalid input should not match"
        );
        return;
    };

    let test_result = pattern.test_input(match_input.clone());
    let exec_result = pattern.exec_input(match_input);
    if matches!(
        &case.expected_match,
        Some(ExpectedMatch::String(value)) if value == "error"
    ) {
        assert!(test_result.is_err() && exec_result.is_err());
        return;
    }

    let expected_match = case.expected_match.map(|expected| match expected {
        ExpectedMatch::String(value) => {
            panic!("URLPattern case {index}: unexpected expected_match string {value}")
        }
        ExpectedMatch::MatchResult(result) => result,
    });
    assert_eq!(
        test_result.unwrap(),
        expected_match.is_some(),
        "URLPattern case {index}: test result"
    );
    let Some(expected_match) = expected_match else {
        assert!(
            exec_result.unwrap().is_none(),
            "URLPattern case {index}: exec should not match"
        );
        return;
    };
    assert_eq!(
        actual_inputs,
        expected_match.inputs.unwrap_or(expected_inputs),
        "URLPattern case {index}: result inputs"
    );

    let empty = &case.exactly_empty_components;
    macro_rules! expected_component {
        ($field:ident) => {
            expected_match
                .$field
                .map(|component| UrlPatternComponentResult {
                    input: component.input,
                    groups: component.groups,
                })
                .unwrap_or_else(|| {
                    let groups = if empty.iter().any(|value| value == stringify!($field)) {
                        HashMap::new()
                    } else {
                        HashMap::from([("0".to_owned(), Some(String::new()))])
                    };
                    UrlPatternComponentResult {
                        input: String::new(),
                        groups,
                    }
                })
        };
    }
    let expected = UrlPatternResult {
        protocol: expected_component!(protocol),
        username: expected_component!(username),
        password: expected_component!(password),
        hostname: expected_component!(hostname),
        port: expected_component!(port),
        pathname: expected_component!(pathname),
        search: expected_component!(search),
        hash: expected_component!(hash),
    };
    assert_eq!(
        exec_result.unwrap().unwrap(),
        expected,
        "URLPattern case {index}: capture result"
    );
}

fn init_implies_wildcard(input: &StringOrInit<'_>, field: &str) -> bool {
    let StringOrInit::Init(init) = input else {
        return false;
    };
    match field {
        "protocol" | "username" | "password" => false,
        "hostname" => init.protocol.is_some(),
        "port" => init.protocol.is_some() || init.hostname.is_some(),
        "pathname" => init.protocol.is_some() || init.hostname.is_some() || init.port.is_some(),
        "search" => {
            init.protocol.is_some()
                || init.hostname.is_some()
                || init.port.is_some()
                || init.pathname.is_some()
        }
        "hash" => {
            init.protocol.is_some()
                || init.hostname.is_some()
                || init.port.is_some()
                || init.pathname.is_some()
                || init.search.is_some()
        }
        _ => unreachable!(),
    }
}

#[test]
fn url_pattern_wpt() {
    let source = replace_unpaired_surrogate_escapes(include_str!("wpt/urlpatterntestdata.json"));
    let cases: Vec<TestCase<'_>> = serde_json::from_str(&source).unwrap();
    assert_eq!(cases.len(), 369, "the pinned corpus changed");
    for (index, case) in cases.into_iter().enumerate() {
        run_case(index, case);
    }
}

fn replace_unpaired_surrogate_escapes(source: &str) -> String {
    let bytes = source.as_bytes();
    let mut output = String::with_capacity(source.len());
    let mut index = 0;
    while index < bytes.len() {
        let is_escape =
            index + 6 <= bytes.len() && bytes[index] == b'\\' && bytes[index + 1] == b'u';
        if !is_escape {
            let character = source[index..].chars().next().expect("valid UTF-8 fixture");
            output.push(character);
            index += character.len_utf8();
            continue;
        }
        let Some(code_unit) = parse_hex_u16(&bytes[index + 2..index + 6]) else {
            output.push('\\');
            index += 1;
            continue;
        };
        if (0xd800..=0xdbff).contains(&code_unit) {
            let paired = index + 12 <= bytes.len()
                && bytes[index + 6] == b'\\'
                && bytes[index + 7] == b'u'
                && parse_hex_u16(&bytes[index + 8..index + 12])
                    .is_some_and(|low| (0xdc00..=0xdfff).contains(&low));
            if paired {
                output.push_str(&source[index..index + 12]);
                index += 12;
            } else {
                output.push_str("\\uFFFD");
                index += 6;
            }
        } else if (0xdc00..=0xdfff).contains(&code_unit) {
            output.push_str("\\uFFFD");
            index += 6;
        } else {
            output.push_str(&source[index..index + 6]);
            index += 6;
        }
    }
    output
}

fn parse_hex_u16(input: &[u8]) -> Option<u16> {
    input.iter().try_fold(0_u16, |value, byte| {
        let digit = match byte {
            b'0'..=b'9' => u16::from(byte - b'0'),
            b'a'..=b'f' => u16::from(byte - b'a' + 10),
            b'A'..=b'F' => u16::from(byte - b'A' + 10),
            _ => return None,
        };
        Some(value * 16 + digit)
    })
}
