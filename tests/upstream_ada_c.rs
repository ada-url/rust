#![cfg(feature = "std")]

use ada_url::{HostType, Idna, SchemeType, Url, UrlSearchParams};

#[test]
fn ada_c_ada_parse() {
    let input = "https://username:password@www.google.com:8080/pathname?query=true#hash-exists";
    let url = Url::parse(input, None);
    assert!(url.is_ok());
}

#[test]
fn ada_c_ada_parse_with_base() {
    let input = "/hello";
    let base = "https://username:password@www.google.com:8080/pathname?query=true#hash-exists";
    let url = Url::parse(input, Some(base));
    assert!(url.is_ok());
}

#[test]
fn ada_c_getters() {
    let input = "https://username:password@www.google.com:8080/pathname?query=true#hash-exists";
    let url = Url::parse(input, None).expect("valid url");

    assert_eq!(url.origin(), "https://www.google.com:8080");
    assert_eq!(url.href(), input);
    assert_eq!(url.username(), "username");
    assert_eq!(url.password(), "password");
    assert_eq!(url.port(), "8080");
    assert_eq!(url.hash(), "#hash-exists");
    assert_eq!(url.host(), "www.google.com:8080");
    assert_eq!(url.hostname(), "www.google.com");
    assert_eq!(url.pathname(), "/pathname");
    assert_eq!(url.search(), "?query=true");
    assert_eq!(url.protocol(), "https:");
}

#[test]
fn ada_c_setters() {
    let input = "https://username:password@www.google.com:8080/pathname?query=true#hash-exists";
    let mut url = Url::parse(input, None).expect("valid url");

    url.set_href("https://www.yagiz.co").unwrap();
    assert_eq!(url.href(), "https://www.yagiz.co/");

    url.set_username(Some("new-username")).unwrap();
    assert_eq!(url.username(), "new-username");

    url.set_password(Some("new-password")).unwrap();
    assert_eq!(url.password(), "new-password");

    url.set_port(Some("4242")).unwrap();
    assert_eq!(url.port(), "4242");
    url.set_port(None).unwrap();
    assert_eq!(url.port(), "");
    assert!(!url.has_port());

    url.set_hash(Some("new-hash"));
    assert_eq!(url.hash(), "#new-hash");
    url.set_hash(None);
    assert!(!url.has_hash());

    url.set_hostname(Some("new-host")).unwrap();
    assert_eq!(url.hostname(), "new-host");

    url.set_host(Some("changed-host:9090")).unwrap();
    assert_eq!(url.host(), "changed-host:9090");

    url.set_pathname(Some("new-pathname")).unwrap();
    assert_eq!(url.pathname(), "/new-pathname");

    url.set_search(Some("new-search"));
    assert_eq!(url.search(), "?new-search");
    url.set_search(None);
    assert_eq!(url.search(), "");

    url.set_protocol("wss").unwrap();
    assert_eq!(url.protocol(), "wss:");

    assert_eq!(url.host_type(), HostType::Domain);
}

#[test]
fn ada_c_can_parse() {
    let input = "https://www.google.com";
    let path = "/hello-world";

    assert!(Url::can_parse(input, None));
    assert!(!Url::can_parse(path, None));
    assert!(Url::can_parse(path, Some(input)));
}

#[test]
fn ada_c_ada_url_components() {
    let input = "https://www.google.com";
    let url = Url::parse(input, None).expect("valid url");
    let components = url.components();

    assert_eq!(components.protocol_end, 6);
    assert_eq!(components.port, None);
    assert_eq!(components.search_start, None);
    assert_eq!(components.hash_start, None);
}

#[test]
fn ada_c_ada_copy() {
    let lemire_blog = "https://lemire.me";
    let anonrig_blog = "https://yagiz.co";
    let first = Url::parse(lemire_blog, None).expect("valid url");
    let mut second = first.clone();

    second.set_href(anonrig_blog).unwrap();

    assert_eq!(first.href(), "https://lemire.me/");
    assert_eq!(second.href(), "https://yagiz.co/");
}

#[test]
fn ada_c_ada_idna() {
    let ascii_input = "straße.de";
    let unicode_input = "xn--strae-oqa.de";

    let ascii = Idna::ascii(ascii_input);
    assert_eq!(ascii, unicode_input);

    let unicode = Idna::unicode(unicode_input);
    assert_eq!(unicode, ascii_input);
}

#[test]
fn ada_c_ada_clear_hash() {
    let input = "https://www.google.com/hello-world?query=1#";
    let mut out = Url::parse(input, None).expect("valid url");

    out.set_hash(None);
    assert_eq!(out.hash(), "");
    assert!(!out.has_hash());
    assert_eq!(out.href(), "https://www.google.com/hello-world?query=1");
}

#[test]
fn ada_c_ada_clear_search() {
    let input = "https://www.google.com/hello-world?#hash";
    let mut out = Url::parse(input, None).expect("valid url");

    out.set_search(None);
    assert_eq!(out.search(), "");
    assert!(!out.has_search());
    assert_eq!(out.href(), "https://www.google.com/hello-world#hash");
}

#[test]
fn ada_c_ada_get_scheme_type() {
    let cases = [
        ("http://www.google.com", SchemeType::Http),
        ("notspecial://www.google.com", SchemeType::NotSpecial),
        ("https://www.google.com", SchemeType::Https),
        ("ws://www.google.com/ws", SchemeType::Ws),
        ("ftp://www.google.com/file.txt", SchemeType::Ftp),
        ("wss://www.google.com/wss", SchemeType::Wss),
        ("file:///foo/bar", SchemeType::File),
    ];

    for (input, expected) in cases {
        let out = Url::parse(input, None).expect("valid url");
        assert_eq!(out.scheme_type(), expected);
    }
}

#[test]
fn ada_c_ada_url_search_params() {
    let input = "a=b&c=d&c=e&f=g";
    let mut out = UrlSearchParams::parse(input).expect("search params");

    assert_eq!(out.len(), 4);

    let key = "key1";
    let value = "value1";
    let value2 = "value2";

    out.append(key, value);
    assert_eq!(out.len(), 5);

    out.set(key, value2);
    assert_eq!(out.len(), 5);

    assert!(out.contains_key(key));
    assert!(!out.contains(key, value));
    assert!(out.contains(key, value2));

    let all = out.get_all(key);
    assert_eq!(all.len(), 1);
    assert_eq!(all.get(0), Some("value2"));
    drop(all);

    {
        let keys: Vec<_> = out.keys().collect();
        let values: Vec<_> = out.values().collect();
        let entries: Vec<_> = out.entries().collect();

        assert_eq!(keys, vec!["a", "c", "c", "f", "key1"]);
        assert_eq!(values, vec!["b", "d", "e", "g", "value2"]);

        assert_eq!(entries[0], ("a", "b"));
        assert_eq!(entries[1], ("c", "d"));
    }

    out.remove_key(key);
    out.remove(key, value);
    assert_eq!(out.to_string(), "a=b&c=d&c=e&f=g");
}
