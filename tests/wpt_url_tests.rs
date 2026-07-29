//! Pinned WHATWG URL parser conformance tests.

use ada_url::{PercentEncodeSet, Url, domain_to_ascii, percent_encode};
use serde_json::{Map, Value};

const URL_SOURCES: &[&str] = &[
    "tests/wpt/urltestdata.json",
    "tests/wpt/ada_extra_urltestdata.json",
    "tests/wpt/ada_long_urltestdata.json",
];
const SETTER_SOURCES: &[&str] = &[
    "tests/wpt/setters_tests.json",
    "tests/wpt/ada_extra_setters_tests.json",
];

fn read_json(path: &str) -> Value {
    let source = std::fs::read_to_string(path).unwrap_or_else(|error| {
        panic!("failed to read {path}: {error}");
    });
    serde_json::from_str(&source).unwrap_or_else(|error| {
        panic!("failed to parse {path}: {error}");
    })
}

fn read_json_with_replacement(path: &str) -> Value {
    let source = std::fs::read_to_string(path).unwrap_or_else(|error| {
        panic!("failed to read {path}: {error}");
    });
    let source = replace_unpaired_surrogate_escapes(&source);
    serde_json::from_str(&source).unwrap_or_else(|error| {
        panic!("failed to parse {path} after surrogate replacement: {error}");
    })
}

fn replace_unpaired_surrogate_escapes(source: &str) -> String {
    let bytes = source.as_bytes();
    let mut output = String::with_capacity(source.len());
    let mut index = 0;
    while index < bytes.len() {
        let is_escape =
            index + 6 <= bytes.len() && bytes[index] == b'\\' && bytes[index + 1] == b'u';
        if !is_escape {
            let character = source[index..].chars().next().expect("valid UTF-8 source");
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
            let has_low = index + 12 <= bytes.len()
                && bytes[index + 6] == b'\\'
                && bytes[index + 7] == b'u'
                && parse_hex_u16(&bytes[index + 8..index + 12])
                    .is_some_and(|low| (0xdc00..=0xdfff).contains(&low));
            if has_low {
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

fn object<'a>(value: &'a Value, context: &str) -> &'a Map<String, Value> {
    value
        .as_object()
        .unwrap_or_else(|| panic!("{context} is not an object"))
}

fn string<'a>(value: &'a Map<String, Value>, key: &str) -> &'a str {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("missing string field {key}"))
}

fn optional_string<'a>(value: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

fn assert_field(url: &Url, field: &str, expected: &str, context: &str) {
    let actual = match field {
        "protocol" => url.protocol(),
        "username" => url.username(),
        "password" => url.password(),
        "host" => url.host(),
        "hostname" => url.hostname(),
        "port" => url.port(),
        "pathname" => url.pathname(),
        "search" => url.search(),
        "hash" => url.hash(),
        "href" => url.href(),
        other => panic!("{context}: unsupported expected field {other}"),
    };
    assert_eq!(actual, expected, "{context}: field={field}");
}

#[test]
fn urltestdata() {
    for source in URL_SOURCES {
        let document = read_json(source);
        let entries = document
            .as_array()
            .unwrap_or_else(|| panic!("{source} root is not an array"));
        let mut executed = 0_usize;

        for (index, entry) in entries.iter().enumerate() {
            if entry.is_string() {
                continue;
            }
            let case = object(entry, "URL test entry");
            let input = string(case, "input");
            let base = optional_string(case, "base");
            let should_fail = case
                .get("failure")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let parsed = Url::parse_with_base(input, base);

            if should_fail {
                assert!(
                    parsed.is_err(),
                    "{source}:{index}: expected failure for input={input:?}, base={base:?}"
                );
                continue;
            }

            let url = parsed.unwrap_or_else(|error| {
                panic!(
                    "{source}:{index}: expected success for input={input:?}, \
                     base={base:?}, error={error}"
                )
            });
            let context = || format!("{source}:{index}: input={input:?}, base={base:?}");
            assert_eq!(url.protocol(), string(case, "protocol"), "{}", context());
            assert_eq!(url.username(), string(case, "username"), "{}", context());
            assert_eq!(url.password(), string(case, "password"), "{}", context());
            assert_eq!(url.host(), string(case, "host"), "{}", context());
            assert_eq!(url.hostname(), string(case, "hostname"), "{}", context());
            assert_eq!(url.port(), string(case, "port"), "{}", context());
            assert_eq!(url.pathname(), string(case, "pathname"), "{}", context());
            assert_eq!(url.search(), string(case, "search"), "{}", context());
            assert_eq!(url.hash(), string(case, "hash"), "{}", context());
            assert_eq!(url.href(), string(case, "href"), "{}", context());
            if let Some(expected_origin) = optional_string(case, "origin") {
                assert_eq!(url.origin(), expected_origin, "{}", context());
            }
            assert!(url.validate(), "{}: invalid internal offsets", context());
            executed += 1;
        }

        assert!(executed > 0, "no URL tests executed from {source}");
    }
}

#[test]
fn setters() {
    for source in SETTER_SOURCES {
        let document = read_json(source);
        let categories = document
            .as_object()
            .unwrap_or_else(|| panic!("{source} root is not an object"));

        for (category, cases) in categories {
            if category == "comment" {
                continue;
            }
            let cases = cases
                .as_array()
                .unwrap_or_else(|| panic!("{source}:{category} is not an array"));
            for (index, case) in cases.iter().enumerate() {
                let case = object(case, "setter entry");
                let href = string(case, "href");
                let new_value = string(case, "new_value");
                let mut url = Url::parse(href, None).unwrap_or_else(|error| {
                    panic!("{source}:{category}:{index}: invalid fixture URL {href:?}: {error}")
                });

                let _result = match category.as_str() {
                    "protocol" => url.set_protocol(new_value),
                    "username" => url.set_username(Some(new_value)),
                    "password" => url.set_password(Some(new_value)),
                    "host" => url.set_host(Some(new_value)),
                    "hostname" => url.set_hostname(Some(new_value)),
                    "port" => url.set_port(Some(new_value)),
                    "pathname" => url.set_pathname(Some(new_value)),
                    "search" => {
                        url.set_search(Some(new_value));
                        Ok(())
                    }
                    "hash" => {
                        url.set_hash(Some(new_value));
                        Ok(())
                    }
                    "href" => url.set_href(new_value),
                    other => panic!("{source}: unsupported setter category {other}"),
                };

                let expected = case
                    .get("expected")
                    .and_then(Value::as_object)
                    .unwrap_or_else(|| {
                        panic!("{source}:{category}:{index}: missing expected object")
                    });
                let context =
                    format!("{source}:{category}:{index}: href={href:?}, new_value={new_value:?}");
                for (field, expected) in expected {
                    if let Some(expected) = expected.as_str() {
                        assert_field(&url, field, expected, &context);
                    }
                }
                assert!(url.validate(), "{context}: invalid internal offsets");
            }
        }
    }
}

#[test]
fn idna_test_v2() {
    let source = "tests/wpt/IdnaTestV2.json";
    let document = read_json_with_replacement(source);
    let entries = document
        .as_array()
        .unwrap_or_else(|| panic!("{source} root is not an array"));
    let mut executed = 0_usize;

    for (index, entry) in entries.iter().enumerate() {
        if entry.is_string() {
            continue;
        }
        let case = object(entry, "IDNA entry");
        let input = string(case, "input");
        let expected = case.get("output").unwrap_or_else(|| {
            panic!("{source}:{index}: missing output");
        });
        let parsed = domain_to_ascii(input);
        match expected.as_str() {
            Some(expected) => {
                let output = parsed.unwrap_or_else(|error| {
                    panic!("{source}:{index}: input={input:?} should pass: {error}")
                });
                assert_eq!(output, expected, "{source}:{index}: input={input:?}");
            }
            None => assert!(
                parsed.is_err(),
                "{source}:{index}: input={input:?} should fail"
            ),
        }
        executed += 1;
    }
    assert!(executed > 2_000, "unexpectedly small IDNA corpus");
}

#[test]
fn toascii() {
    let source = "tests/wpt/toascii.json";
    let document = read_json_with_replacement(source);
    let entries = document
        .as_array()
        .unwrap_or_else(|| panic!("{source} root is not an array"));
    let mut executed = 0_usize;

    for (index, entry) in entries.iter().enumerate() {
        if entry.is_string() {
            continue;
        }
        let case = object(entry, "toASCII entry");
        let input = string(case, "input");
        let expected = case.get("output").unwrap_or_else(|| {
            panic!("{source}:{index}: missing output");
        });
        let converted = domain_to_ascii(input);
        match expected.as_str() {
            Some(expected) => assert_eq!(
                converted.unwrap_or_else(|error| {
                    panic!("{source}:{index}: input={input:?} should pass: {error}")
                }),
                expected,
                "{source}:{index}: input={input:?}"
            ),
            None => assert!(
                converted.is_err(),
                "{source}:{index}: input={input:?} should fail"
            ),
        }
        executed += 1;
    }
    assert!(executed > 50, "unexpectedly small toASCII corpus");
}

#[test]
fn percent_encoding() {
    let cases = read_json("tests/wpt/percent-encoding.json");
    let cases = cases.as_array().expect("percent-encoding root is an array");
    let mut executed = 0;
    for case in cases {
        let Some(case) = case.as_object() else {
            continue;
        };
        let input = string(case, "input");
        let expected = case
            .get("output")
            .and_then(Value::as_object)
            .and_then(|output| output.get("utf-8"))
            .and_then(Value::as_str)
            .expect("UTF-8 percent-encoding output");
        assert_eq!(
            percent_encode(input, PercentEncodeSet::Query),
            expected,
            "percent-encoding input {input:?}"
        );
        executed += 1;
    }
    assert_eq!(executed, 7, "the pinned percent-encoding corpus changed");
}

#[test]
fn verify_dns_length() {
    let source = "tests/wpt/verifydnslength_tests.json";
    let document = read_json(source);
    let entries = document
        .as_array()
        .unwrap_or_else(|| panic!("{source} root is not an array"));
    let mut executed = 0_usize;

    for (index, entry) in entries.iter().enumerate() {
        if entry.is_string() {
            continue;
        }
        let case = object(entry, "DNS length entry");
        let input = string(case, "input");
        let message = string(case, "message");
        let should_fail = case
            .get("failure")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let url = Url::parse(input, None).unwrap_or_else(|error| {
            panic!("{source}:{index}: could not parse input={input:?}: {error}")
        });
        assert_eq!(
            url.has_valid_domain(),
            !should_fail,
            "{source}:{index}: {message}"
        );
        executed += 1;
    }

    assert_eq!(executed, 17, "the pinned DNS-length corpus changed");
}
