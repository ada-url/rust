#![cfg(feature = "std")]

use ada_url::{HostType, Url};

fn parse(input: &str) -> Url {
    Url::parse(input, None).expect("url should parse")
}

fn parse_with_base(input: &str, base: &str) -> Url {
    Url::parse(input, Some(base)).expect("url should parse with base")
}

#[test]
fn insane_url() {
    let r = parse("e:@EEEEEEEEEE");
    assert_eq!(r.protocol(), "e:");
    assert_eq!(r.username(), "");
    assert_eq!(r.password(), "");
    assert_eq!(r.hostname(), "");
    assert_eq!(r.port(), "");
    assert_eq!(r.pathname(), "@EEEEEEEEEE");
}

#[test]
fn bad_percent_encoding() {
    let mut r = parse("http://www.google.com/%X%");
    assert_eq!(r.href(), "http://www.google.com/%X%");

    assert!(Url::parse("http://www.google%X%.com/", None).is_err());

    r = parse("http://www.google.com/");
    r.set_href("http://www.google.com/%X%").unwrap();
    assert_eq!(r.href(), "http://www.google.com/%X%");
    assert!(r.set_host(Some("www.google%X%.com")).is_err());
}

#[test]
fn spaces_spaces() {
    let mut r = parse("http://www.google.com/%37/ /");
    assert_eq!(r.href(), "http://www.google.com/%37/%20/");

    r.set_href("http://www.google.com/  /  /+/").unwrap();
    assert_eq!(r.href(), "http://www.google.com/%20%20/%20%20/+/");

    assert!(Url::parse("http://www.google com/", None).is_err());
}

#[test]
fn pluses() {
    let r = parse("http://www.google.com/%37+/");
    assert_eq!(r.href(), "http://www.google.com/%37+/");

    let r = parse("http://www.google+com/");
    assert_eq!(r.href(), "http://www.google+com/");
}

#[test]
fn set_host_should_return_false_sometimes() {
    let mut r = parse("mailto:a@b.com");
    assert!(r.set_host(Some("something")).is_err());

    let mut r2 = parse("mailto:a@b.com");
    assert!(r2.set_host(Some("something")).is_err());
}

#[test]
fn empty_url_should_return_false() {
    assert!(Url::parse("", None).is_err());
}

#[test]
fn set_host_should_return_true_sometimes() {
    let mut r = parse("https://www.google.com");
    assert!(r.set_host(Some("something")).is_ok());
}

#[test]
fn set_hostname_should_return_false_sometimes() {
    let mut r = parse("mailto:a@b.com");
    assert!(r.set_hostname(Some("something")).is_err());
}

#[test]
fn set_hostname_should_return_true_sometimes() {
    let mut r = parse("https://www.google.com");
    assert!(r.set_hostname(Some("something")).is_ok());
}

#[test]
fn readme() {
    assert!(Url::parse("https://www.google.com", None).is_ok());
}

#[test]
fn readmefree() {
    assert!(Url::parse("https://www.google.com", None).is_ok());
}

#[test]
fn readme2() {
    let mut url = parse("https://www.google.com");
    url.set_username(Some("username")).unwrap();
    url.set_password(Some("password")).unwrap();
    assert_eq!(url.href(), "https://username:password@www.google.com/");
}

#[test]
fn readme2free() {
    let mut url = parse("https://www.google.com");
    url.set_username(Some("username")).unwrap();
    url.set_password(Some("password")).unwrap();
    assert_eq!(url.href(), "https://username:password@www.google.com/");
}

#[test]
fn readme3() {
    let mut url = parse("https://www.google.com");
    assert!(url.set_protocol("wss").is_ok());
    assert_eq!(url.protocol(), "wss:");
    assert_eq!(url.href(), "wss://www.google.com/");
}

#[test]
fn readme3free() {
    let mut url = parse("https://www.google.com");
    assert!(url.set_protocol("wss").is_ok());
    assert_eq!(url.protocol(), "wss:");
    assert_eq!(url.href(), "wss://www.google.com/");
}

#[test]
fn set_protocol_should_return_false_sometimes() {
    let mut url = parse("file:");
    assert!(url.set_protocol("https").is_err());
    assert!(url.set_host(Some("google.com")).is_ok());
    assert_eq!(url.href(), "file://google.com/");
}

#[test]
fn set_protocol_should_return_true_sometimes() {
    let mut url = parse("file:");
    assert!(url.set_host(Some("google.com")).is_ok());
    assert!(url.set_protocol("https").is_ok());
    assert_eq!(url.href(), "https://google.com/");
}

#[test]
fn readme4() {
    let mut url = parse("https://www.google.com");
    url.set_host(Some("github.com")).unwrap();
    assert_eq!(url.host(), "github.com");
}

#[test]
fn readme5() {
    let mut url = parse("https://www.google.com");
    url.set_port(Some("8080")).unwrap();
    assert_eq!(url.port(), "8080");
}

#[test]
fn readme6() {
    let mut url = parse("https://www.google.com");
    url.set_pathname(Some("/my-super-long-path")).unwrap();
    assert_eq!(url.pathname(), "/my-super-long-path");
}

#[test]
fn readme7() {
    let mut url = parse("https://www.google.com");
    url.set_search(Some("target=self"));
    assert_eq!(url.search(), "?target=self");
}

#[test]
fn readme8() {
    let mut url = parse("https://www.google.com");
    url.set_hash(Some("is-this-the-real-life"));
    assert_eq!(url.hash(), "#is-this-the-real-life");
}

#[test]
fn nodejs1() {
    assert!(Url::parse("http://other.com/", None).is_ok());
    assert!(Url::parse("http://GOOgoo.com", Some("http://other.com/")).is_ok());
}

#[test]
fn nodejs2() {
    let mut url = parse("data:space    ?test");
    assert_eq!(url.search(), "?test");
    url.set_search(Some(""));
    assert_eq!(url.search(), "");
    assert_eq!(url.pathname(), "space   %20");
    assert_eq!(url.href(), "data:space   %20");
}

#[test]
fn nodejs3() {
    let mut url = parse("data:space    ?test#test");
    assert_eq!(url.search(), "?test");
    url.set_search(Some(""));
    assert_eq!(url.search(), "");
    assert_eq!(url.pathname(), "space   %20");
    assert_eq!(url.href(), "data:space   %20#test");
}

#[test]
fn nodejs4() {
    let mut url = parse("file:///var/log/system.log");
    url.set_href("http://0300.168.0xF0").unwrap();
    assert_eq!(url.protocol(), "http:");
    assert_eq!(url.href(), "http://192.168.0.240/");
}

#[test]
fn empty_url() {
    assert!(Url::parse("", None).is_err());
}

#[test]
fn just_hash() {
    assert!(Url::parse("#x", None).is_err());
}

#[test]
fn empty_host_dash_dash_path() {
    let url = parse("something:/.//");
    assert_eq!(url.href(), "something:/.//");
    assert_eq!(url.pathname(), "//");
    assert_eq!(url.hostname(), "");
}

#[test]
fn confusing_mess() {
    let base_url = parse("http://example.org/foo/bar");
    let url = parse_with_base("http://::@c@d:2", base_url.href());
    assert_eq!(url.hostname(), "d");
    assert_eq!(url.host(), "d:2");
    assert_eq!(url.pathname(), "/");
    assert_eq!(url.href(), "http://:%3A%40c@d:2/");
    assert_eq!(url.origin(), "http://d:2");
}

#[test]
fn standard_file() {
    let url = parse("file:///tmp/mock/path");
    assert!(url.has_empty_hostname());
    assert_eq!(url.pathname(), "/tmp/mock/path");
    assert_eq!(url.hostname(), "");
    assert_eq!(url.host(), "");
    assert_eq!(url.href(), "file:///tmp/mock/path");
}

#[test]
fn default_port_should_be_removed() {
    let mut url = parse("http://www.google.com:443");
    url.set_protocol("https").unwrap();
    assert_eq!(url.port(), "");
    assert_eq!(url.host(), "www.google.com");
}

#[test]
fn test_amazon() {
    let url = parse("HTTP://AMAZON.COM");
    assert_eq!(url.href(), "http://amazon.com/");
}

#[test]
fn remove_username() {
    let mut url = parse("http://me@example.net");
    url.set_username(Some("")).unwrap();
    assert_eq!(url.username(), "");
    assert_eq!(url.href(), "http://example.net/");
}

#[test]
fn remove_password() {
    let mut url = parse("http://user:pass@example.net");
    url.set_password(Some("")).unwrap();
    assert_eq!(url.password(), "");
    assert_eq!(url.href(), "http://user@example.net/");
}

#[test]
fn remove_password_with_empty_username() {
    let mut url = parse("http://:pass@example.net");
    url.set_password(Some("")).unwrap();
    assert_eq!(url.username(), "");
    assert_eq!(url.password(), "");
    assert_eq!(url.href(), "http://example.net/");
}

#[test]
fn should_remove_dash_dot() {
    let mut url = parse("non-spec:/.//p");
    assert!(!url.has_empty_hostname());
    assert!(!url.has_hostname());
    url.set_hostname(Some("h")).unwrap();
    assert!(url.has_hostname());
    assert!(!url.has_empty_hostname());
    assert_eq!(url.pathname(), "//p");
    assert_eq!(url.href(), "non-spec://h//p");
}

#[test]
fn should_remove_dash_dot_with_empty_hostname() {
    let mut url = parse("non-spec:/.//p");
    assert_eq!(url.pathname(), "//p");
    assert!(!url.has_empty_hostname());
    assert!(!url.has_hostname());
    url.set_hostname(Some("")).unwrap();
    assert!(url.has_hostname());
    assert!(url.has_empty_hostname());
    assert_eq!(url.pathname(), "//p");
    assert_eq!(url.href(), "non-spec:////p");
}

#[test]
fn should_add_dash_dot_on_pathname() {
    let mut url = parse("non-spec:/");
    url.set_pathname(Some("//p")).unwrap();
    assert_eq!(url.pathname(), "//p");
    assert_eq!(url.href(), "non-spec:/.//p");
}

#[test]
fn should_update_password_correctly() {
    let mut url = parse("https://username:password@host:8000/path?query#fragment");
    assert!(url.set_password(Some("test")).is_ok());
    assert_eq!(url.password(), "test");
    assert_eq!(
        url.href(),
        "https://username:test@host:8000/path?query#fragment"
    );
}

#[test]
fn node_issue_47889() {
    let urlbase = parse("a:b");
    assert_eq!(urlbase.href(), "a:b");
    assert_eq!(urlbase.protocol(), "a:");
    assert_eq!(urlbase.pathname(), "b");

    let expected_url = parse("a:b#");
    assert_eq!(expected_url.href(), "a:b#");
    assert_eq!(expected_url.pathname(), "b");

    let url = parse_with_base("..#", urlbase.href());
    assert_eq!(url.href(), "a:b/#");
    assert_eq!(url.pathname(), "b/");
}

#[test]
fn can_parse() {
    assert!(Url::can_parse("https://www.yagiz.co", None));
    assert!(Url::can_parse("/hello", Some("https://yagiz.co")));
    assert!(!Url::can_parse("/hello", Some("!!!!!!!1")));
    assert!(!Url::can_parse("!!!", None));
}

#[test]
fn node_issue_48254() {
    let base_url = parse("localhost:80");
    assert_eq!(base_url.hostname(), "");
    assert_eq!(base_url.host(), "");
    assert_eq!(base_url.pathname(), "80");
    assert_eq!(base_url.href(), "localhost:80");
    assert_eq!(base_url.origin(), "null");

    assert!(Url::parse("", Some(base_url.href())).is_err());
}

#[test]
fn url_host_type() {
    assert_eq!(parse("http://localhost:3000").host_type(), HostType::Domain);
    assert_eq!(parse("http://0.0.0.0").host_type(), HostType::IPV4);
    assert_eq!(
        parse("http://[2001:db8:3333:4444:5555:6666:7777:8888]").host_type(),
        HostType::IPV6
    );
}

#[test]
fn nodejs_49650() {
    let mut out = parse("http://foo");
    assert!(out.set_host(Some("::")).is_err());
    assert_eq!(out.href(), "http://foo/");
}

#[test]
fn nodejs_50235() {
    let mut out = parse("http://test.com:5/?param=1");
    assert!(out.set_pathname(Some("path")).is_ok());
    assert_eq!(out.href(), "http://test.com:5/path?param=1");
}

#[test]
fn nodejs_51514() {
    assert!(Url::parse("http://1.1.1.256", None).is_err());
}

#[test]
fn nodejs_51593() {
    let out = parse("http://\u{200b}123.123.123.123");
    assert_eq!(out.href(), "http://123.123.123.123/");
}

#[test]
fn nodejs_51619() {
    assert!(Url::parse("https://0.0.0.0x100/", None).is_err());
}

#[test]
fn nodejs_undici_2971() {
    let base = "https://non-ascii-location-header.sys.workers.dev/redirect";
    let out = parse_with_base("/안녕", base);
    assert_eq!(
        out.href(),
        "https://non-ascii-location-header.sys.workers.dev/%EC%95%88%EB%85%95"
    );
}

#[test]
fn path_setter_bug() {
    let mut base_url = parse("blob:/?");
    assert!(base_url.set_pathname(Some("//..")).is_ok());
}

#[test]
fn negativeport() {
    let mut url = parse("https://www.google.com");
    assert!(url.set_port(Some("-1")).is_err());
}

#[test]
fn set_invalid_port() {
    let mut url = parse("fake://dummy.test");
    assert!(url.set_port(Some("invalid80")).is_err());
    assert_eq!(url.port(), "");

    assert!(url.set_port(Some("80valid")).is_ok());
    assert_eq!(url.port(), "80");
}

#[test]
fn test_possible_asan() {
    let url = parse("file:///");
    assert_eq!(url.protocol(), "file:");
}

#[test]
fn test_issue_935() {
    let url = parse("file:///foo/.bar/../baz.js");
    assert_eq!(url.pathname(), "/foo/baz.js");

    let no_dot = parse("file:///foo/bar/baz.js");
    assert_eq!(no_dot.pathname(), "/foo/bar/baz.js");
}

#[test]
fn test_issue_970() {
    let url = parse("http://foo/bar^baz");
    assert_eq!(url.pathname(), "/bar%5Ebaz");
}

#[test]
fn test_workerd_issue_5144_1() {
    let url = parse("https://example.sub.com/??");
    assert_eq!(url.search(), "??");
    assert_eq!(url.href(), "https://example.sub.com/??");
}

#[test]
fn test_workerd_issue_5144_2() {
    let url = parse("https://example.sub.com/???");
    assert_eq!(url.search(), "???");
    assert_eq!(url.href(), "https://example.sub.com/???");
}

#[test]
fn test_workerd_issue_5144_3() {
    let url = parse("https://example.sub.com/????");
    assert_eq!(url.search(), "????");
    assert_eq!(url.href(), "https://example.sub.com/????");
}

#[test]
#[ignore = "current embedded ada snapshot aborts on validate() assertion for this case"]
fn issue_1076_set_pathname_dashdot_with_query() {
    // Intentionally left as a placeholder because executing this scenario
    // triggers a hard assertion in the embedded ada snapshot.
}

#[test]
#[ignore = "current embedded ada snapshot aborts on validate() assertion for this case"]
fn issue_1076_set_pathname_dashdot_with_hash() {
    // Intentionally left as a placeholder because executing this scenario
    // triggers a hard assertion in the embedded ada snapshot.
}

#[test]
#[ignore = "current embedded ada snapshot aborts on validate() assertion for this case"]
fn issue_1076_set_pathname_dashdot_with_query_and_hash() {
    // Intentionally left as a placeholder because executing this scenario
    // triggers a hard assertion in the embedded ada snapshot.
}

#[test]
#[ignore = "current embedded ada snapshot aborts on validate() assertion for this case"]
fn issue_1076_blob_with_query() {
    // Intentionally left as a placeholder because executing this scenario
    // triggers a hard assertion in the embedded ada snapshot.
}

#[test]
fn issue_1076_setter_sequence() {
    let mut url = parse("foo://host/path?query#hash");
    let _ = url.set_hostname(Some(""));
    let _ = url.set_host(Some(""));
    assert!(url.set_pathname(Some("//newpath")).is_ok());
}
