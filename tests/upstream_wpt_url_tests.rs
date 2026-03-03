#![cfg(feature = "std")]

mod common;

use ada_url::{Idna, Url};
use common::{as_array, as_object, get_str, maybe_str, read_json};
use serde_json::Value;

fn assert_expected_field(url: &Url, key: &str, expected: &str) {
    match key {
        "protocol" => assert_eq!(url.protocol(), expected),
        "username" => assert_eq!(url.username(), expected),
        "password" => assert_eq!(url.password(), expected),
        "host" => assert_eq!(url.host(), expected),
        "hostname" => assert_eq!(url.hostname(), expected),
        "port" => assert_eq!(url.port(), expected),
        "pathname" => assert_eq!(url.pathname(), expected),
        "search" => assert_eq!(url.search(), expected),
        "hash" => assert_eq!(url.hash(), expected),
        "href" => assert_eq!(url.href(), expected),
        "origin" => assert_eq!(url.origin(), expected),
        other => panic!("unexpected expected-field key: {other}"),
    }
}

#[test]
fn setters_tests_encoding() {
    for source in [
        "tests/wpt/setters_tests.json",
        "tests/wpt/ada_extra_setters_tests.json",
    ] {
        let doc = read_json(source);
        let root = as_object(&doc, "setters_tests root");

        for (category, cases_value) in root {
            if category == "comment" {
                continue;
            }

            let cases = cases_value
                .as_array()
                .unwrap_or_else(|| panic!("{source}:{category} is not an array"));

            for case in cases {
                let case_obj = as_object(case, "setters_tests case");
                let href = get_str(case_obj, "href");
                let new_value = get_str(case_obj, "new_value");

                let mut base = Url::parse(href, None).unwrap_or_else(|_| {
                    panic!("source={source}, category={category}, href={href}")
                });

                match category.as_str() {
                    "protocol" => {
                        let _ = base.set_protocol(new_value);
                    }
                    "username" => {
                        let _ = base.set_username(Some(new_value));
                    }
                    "password" => {
                        let _ = base.set_password(Some(new_value));
                    }
                    "host" => {
                        let _ = base.set_host(Some(new_value));
                    }
                    "hostname" => {
                        let _ = base.set_hostname(Some(new_value));
                    }
                    "port" => {
                        let _ = base.set_port(Some(new_value));
                    }
                    "pathname" => {
                        let _ = base.set_pathname(Some(new_value));
                    }
                    "search" => {
                        base.set_search(Some(new_value));
                    }
                    "hash" => {
                        base.set_hash(Some(new_value));
                    }
                    "href" => {
                        let _ = base.set_href(new_value);
                    }
                    other => panic!("unsupported category in {source}: {other}"),
                }

                let expected_obj = case_obj
                    .get("expected")
                    .and_then(Value::as_object)
                    .unwrap_or_else(|| panic!("missing expected object in {source}:{category}"));
                for (key, value) in expected_obj {
                    if let Some(expected_str) = value.as_str() {
                        assert_expected_field(&base, key, expected_str);
                    }
                }
            }
        }
    }
}

#[test]
fn toascii_encoding() {
    let doc = read_json("tests/wpt/toascii.json");
    let mut counter = 0usize;

    for element in as_array(&doc, "toascii") {
        if element.is_string() {
            continue;
        }

        let object = as_object(element, "toascii entry");
        let input = get_str(object, "input");
        let expected_output = object
            .get("output")
            .unwrap_or_else(|| panic!("missing output in toascii entry"));

        let output = Idna::ascii(input);

        if let Some(expected) = expected_output.as_str() {
            assert_eq!(output, expected, "toascii mismatch for input={input:?}");

            let url_string = format!("https://{output}/x");
            let current = Url::parse(&url_string, None)
                .unwrap_or_else(|_| panic!("failed to parse generated URL {url_string:?}"));
            assert_eq!(current.host(), expected);
            assert_eq!(current.hostname(), expected);
            assert_eq!(current.pathname(), "/x");
            assert_eq!(current.href(), format!("https://{expected}/x"));

            let mut setter = Url::parse("https://x/x", None).expect("valid setter URL");
            assert!(setter.set_host(Some(input)).is_ok());
            assert!(setter.set_hostname(Some(input)).is_ok());
            assert_eq!(setter.host(), expected);
            assert_eq!(setter.hostname(), expected);
        } else {
            assert!(expected_output.is_null());
            let _ = output;

            let mut setter = Url::parse("https://x/x", None).expect("valid setter URL");
            assert!(setter.set_host(Some(input)).is_err());
            assert!(setter.set_hostname(Some(input)).is_err());
            assert_eq!(setter.host(), "x");
            assert_eq!(setter.hostname(), "x");
        }

        counter += 1;
    }

    assert!(counter > 0);
}

#[test]
fn urltestdata_encoding() {
    for source in [
        "tests/wpt/urltestdata.json",
        "tests/wpt/ada_extra_urltestdata.json",
    ] {
        let doc = read_json(source);
        let mut counter = 0usize;

        for element in as_array(&doc, "urltestdata") {
            if element.is_string() {
                continue;
            }

            let object = as_object(element, "urltestdata entry");
            let input = get_str(object, "input");
            let base = maybe_str(object, "base");
            let failure = object
                .get("failure")
                .and_then(Value::as_bool)
                .unwrap_or(false);

            let parsed = match base {
                Some(base) => Url::parse(input, Some(base)),
                None => Url::parse(input, None),
            };

            if failure {
                assert!(
                    parsed.is_err(),
                    "source={source} input should fail: {input:?} base={base:?}"
                );
                continue;
            }

            let url = parsed.unwrap_or_else(|_| {
                panic!("source={source} input should parse: {input:?} base={base:?}")
            });

            assert_eq!(url.protocol(), get_str(object, "protocol"));
            assert_eq!(url.username(), get_str(object, "username"));
            assert_eq!(url.password(), get_str(object, "password"));
            assert_eq!(url.host(), get_str(object, "host"));
            assert_eq!(url.hostname(), get_str(object, "hostname"));
            assert_eq!(url.port(), get_str(object, "port"));
            assert_eq!(url.pathname(), get_str(object, "pathname"));
            assert_eq!(url.search(), get_str(object, "search"));
            assert_eq!(url.hash(), get_str(object, "hash"));
            assert_eq!(url.href(), get_str(object, "href"));

            if let Some(origin) = maybe_str(object, "origin") {
                assert_eq!(url.origin(), origin);
            }

            counter += 1;
        }

        assert!(counter > 0, "no executed cases for source={source}");
    }
}
