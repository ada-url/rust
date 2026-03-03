#![cfg(feature = "std")]

mod common;

use ada_url::Url;
use common::{as_array, as_object, get_str, maybe_str, read_json};

#[test]
fn urltestdata_encoding() {
    let doc = read_json("tests/wpt/urltestdata.json");
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
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);

        let parsed = match base {
            Some(base) => Url::parse(input, Some(base)),
            None => Url::parse(input, None),
        };

        if failure {
            assert!(
                parsed.is_err(),
                "input should fail: {input:?}, base={base:?}"
            );
            continue;
        }

        let url = parsed.unwrap_or_else(|_| panic!("input should parse: {input:?}, base={base:?}"));
        let href = url.href();
        let components = url.components();

        assert_eq!(&href[..components.protocol_end as usize], url.protocol());

        if !url.username().is_empty() {
            let username_start = href.find(url.username()).expect("username in href");
            assert_eq!(
                &href[username_start..username_start + url.username().len()],
                url.username()
            );
        }

        if !url.password().is_empty() {
            let password_start = components.username_end as usize + 1;
            assert_eq!(
                &href[password_start..password_start + url.password().len()],
                url.password()
            );
        }

        let mut host_start = components.host_start as usize;
        if url.has_credentials() {
            assert_eq!(href.as_bytes()[host_start], b'@');
            host_start += 1;
        }
        assert_eq!(
            &href[host_start..host_start + url.hostname().len()],
            url.hostname()
        );

        assert_eq!(components.port.is_some(), !url.port().is_empty());

        if !url.pathname().is_empty() {
            let pathname_start = components
                .pathname_start
                .expect("pathname_start should exist") as usize;
            let pathname_end = components
                .search_start
                .or(components.hash_start)
                .map_or(href.len(), |idx| idx as usize);
            assert_eq!(&href[pathname_start..pathname_end], url.pathname());
        }

        if !url.search().is_empty() {
            let search_start = components.search_start.expect("search_start should exist") as usize;
            assert_eq!(
                &href[search_start..search_start + url.search().len()],
                url.search()
            );
        }

        if !url.hash().is_empty() {
            let hash_start = components.hash_start.expect("hash_start should exist") as usize;
            assert_eq!(&href[hash_start..hash_start + url.hash().len()], url.hash());
        }

        counter += 1;
    }

    assert!(counter > 0);
}
