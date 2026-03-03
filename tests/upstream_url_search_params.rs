#![cfg(feature = "std")]

use ada_url::UrlSearchParams;
use std::collections::VecDeque;

#[test]
fn append() {
    let mut search_params = UrlSearchParams::parse("").expect("parse");
    search_params.append("key", "value");
    assert_eq!(search_params.len(), 1);
    assert!(search_params.contains_key("key"));
    search_params.append("key", "value2");
    assert_eq!(search_params.len(), 2);
    assert_eq!(search_params.get_all("key").len(), 2);
}

#[test]
fn to_string() {
    let mut search_params = UrlSearchParams::parse("").expect("parse");
    search_params.append("key1", "value1");
    search_params.append("key2", "value2");
    assert_eq!(search_params.len(), 2);
    assert_eq!(search_params.to_string(), "key1=value1&key2=value2");
}

#[test]
fn with_accents() {
    let mut search_params = UrlSearchParams::parse("").expect("parse");
    search_params.append("key1", "été");
    search_params.append("key2", "Céline Dion++");
    assert_eq!(search_params.len(), 2);
    assert_eq!(
        search_params.to_string(),
        "key1=%C3%A9t%C3%A9&key2=C%C3%A9line+Dion%2B%2B"
    );
    assert_eq!(search_params.get("key1"), Some("été"));
    assert_eq!(search_params.get("key2"), Some("Céline Dion++"));
}

#[test]
fn to_string_serialize_space() {
    let mut params = UrlSearchParams::parse("").expect("parse");
    params.append("a", "b c");
    assert_eq!(params.to_string(), "a=b+c");
    assert_eq!(params.get("a"), Some("b c"));
    params.remove_key("a");
    params.append("a b", "c");
    assert_eq!(params.to_string(), "a+b=c");
    params.remove_key("a b");
    assert_eq!(params.to_string(), "");
    params.append("a", "");
    assert_eq!(params.to_string(), "a=");
    params.append("", "");
    assert_eq!(params.to_string(), "a=&=");
    params.append("", "b");
    assert_eq!(params.to_string(), "a=&=&=b");
}

#[test]
fn to_string_serialize_plus() {
    let mut params = UrlSearchParams::parse("").expect("parse");
    params.append("a", "b+c");
    assert_eq!(params.to_string(), "a=b%2Bc");
    params.remove_key("a");
    params.append("a+b", "c");
    assert_eq!(params.to_string(), "a%2Bb=c");
}

#[test]
fn to_string_serialize_ampersand() {
    let mut params = UrlSearchParams::parse("").expect("parse");
    params.append("&", "a");
    assert_eq!(params.to_string(), "%26=a");
    params.append("b", "&");
    assert_eq!(params.to_string(), "%26=a&b=%26");
}

#[test]
fn set() {
    let mut search_params = UrlSearchParams::parse("").expect("parse");
    search_params.append("key1", "value1");
    search_params.append("key1", "value2");
    assert_eq!(search_params.len(), 2);
    search_params.set("key1", "hello");
    assert_eq!(search_params.len(), 1);
    assert_eq!(search_params.to_string(), "key1=hello");

    search_params.remove_key("key1");
    search_params.append("key1", "value1");
    search_params.append("key1", "value2");
    search_params.append("key2", "value1");
    search_params.set("key1", "value3");
    assert_eq!(search_params.len(), 2);
    assert_eq!(search_params.to_string(), "key1=value3&key2=value1");
    search_params.set("key1", "value4");
    assert_eq!(search_params.to_string(), "key1=value4&key2=value1");
}

#[test]
fn remove() {
    let mut search_params = UrlSearchParams::parse("").expect("parse");
    search_params.append("key1", "value1");
    search_params.append("key1", "value2");
    search_params.append("key2", "value2");
    search_params.remove_key("key2");
    assert_eq!(search_params.len(), 2);
    assert_eq!(search_params.to_string(), "key1=value1&key1=value2");
    search_params.remove("key1", "value2");
    assert_eq!(search_params.len(), 1);
    assert_eq!(search_params.to_string(), "key1=value1");
}

#[test]
fn sort() {
    let mut search_params = UrlSearchParams::parse("").expect("parse");
    search_params.append("bbb", "second");
    search_params.append("aaa", "first");
    search_params.append("ccc", "third");
    assert_eq!(search_params.len(), 3);
    search_params.sort();
    assert_eq!(search_params.to_string(), "aaa=first&bbb=second&ccc=third");
}

#[test]
fn sort_repeated_keys() {
    let mut search_params = UrlSearchParams::parse("z=b&a=b&z=a&a=a").expect("parse");
    assert_eq!(search_params.len(), 4);
    search_params.sort();

    let entries: Vec<_> = search_params.entries().collect();
    assert_eq!(entries[0], ("a", "b"));
    assert_eq!(entries[1], ("a", "a"));
    assert_eq!(entries[2], ("z", "b"));
    assert_eq!(entries[3], ("z", "a"));
}

#[test]
fn sort_unicode_replacement_chars() {
    let mut search_params = UrlSearchParams::parse("�=x&￼&�=a").expect("parse");
    assert_eq!(search_params.len(), 3);
    search_params.sort();

    let entries: Vec<_> = search_params.entries().collect();
    assert_eq!(entries[0], ("￼", ""));
    assert_eq!(entries[1], ("�", "x"));
    assert_eq!(entries[2], ("�", "a"));
}

#[test]
fn sort_unicode_combining_chars() {
    let mut search_params = UrlSearchParams::parse("é&e�&é").expect("parse");
    assert_eq!(search_params.len(), 3);
    search_params.sort();

    let keys: Vec<_> = search_params.keys().collect();
    assert_eq!(keys, vec!["é", "e�", "é"]);
}

#[test]
fn sort_many_params() {
    let mut search_params =
        UrlSearchParams::parse("z=z&a=a&z=y&a=b&z=x&a=c&z=w&a=d&z=v&a=e&z=u&a=f&z=t&a=g")
            .expect("parse");
    assert_eq!(search_params.len(), 14);
    search_params.sort();

    let mut expected = VecDeque::from([
        ("a", "a"),
        ("a", "b"),
        ("a", "c"),
        ("a", "d"),
        ("a", "e"),
        ("a", "f"),
        ("a", "g"),
        ("z", "z"),
        ("z", "y"),
        ("z", "x"),
        ("z", "w"),
        ("z", "v"),
        ("z", "u"),
        ("z", "t"),
    ]);

    for entry in search_params.entries() {
        let check = expected.pop_front().expect("expected entry");
        assert_eq!(check, entry);
    }
    assert!(expected.is_empty());
}

#[test]
fn sort_empty_values() {
    let mut search_params = UrlSearchParams::parse("bbb&bb&aaa&aa=x&aa=y").expect("parse");
    assert_eq!(search_params.len(), 5);
    search_params.sort();

    let entries: Vec<_> = search_params.entries().collect();
    assert_eq!(entries[0], ("aa", "x"));
    assert_eq!(entries[1], ("aa", "y"));
    assert_eq!(entries[2], ("aaa", ""));
    assert_eq!(entries[3], ("bb", ""));
    assert_eq!(entries[4], ("bbb", ""));
}

#[test]
fn sort_empty_keys() {
    let mut search_params = UrlSearchParams::parse("z=z&=f&=t&=x").expect("parse");
    assert_eq!(search_params.len(), 4);
    search_params.sort();

    let entries: Vec<_> = search_params.entries().collect();
    assert_eq!(entries[0], ("", "f"));
    assert_eq!(entries[1], ("", "t"));
    assert_eq!(entries[2], ("", "x"));
    assert_eq!(entries[3], ("z", "z"));
}

#[test]
fn sort_unicode_emoji() {
    let mut search_params = UrlSearchParams::parse("a🌈&a💩").expect("parse");
    assert_eq!(search_params.len(), 2);
    search_params.sort();

    let keys: Vec<_> = search_params.keys().collect();
    assert_eq!(keys, vec!["a🌈", "a💩"]);
}

#[test]
fn string_constructor() {
    let p = UrlSearchParams::parse("?a=b").expect("parse");
    assert_eq!(p.to_string(), "a=b");
}

#[test]
fn string_constructor_with_empty_input() {
    let p = UrlSearchParams::parse("").expect("parse");
    assert_eq!(p.to_string(), "");
    assert_eq!(p.len(), 0);
}

#[test]
fn string_constructor_without_value() {
    let p = UrlSearchParams::parse("a=b&c").expect("parse");
    assert_eq!(p.to_string(), "a=b&c=");
}

#[test]
fn string_constructor_with_edge_cases() {
    let p = UrlSearchParams::parse("&a&&& &&&&&a+b=& c&m%c3%b8%c3%b8").expect("parse");
    assert!(p.contains_key("a"));
    assert!(p.contains_key("a b"));
    assert!(p.contains_key(" "));
    assert!(!p.contains_key("c"));
    assert!(p.contains_key(" c"));
    assert!(p.contains_key("møø"));
}

#[test]
fn has() {
    let search_params = UrlSearchParams::parse("key1=value1&key2=value2").expect("parse");
    assert!(search_params.contains_key("key1"));
    assert!(search_params.contains_key("key2"));
    assert!(search_params.contains("key1", "value1"));
    assert!(search_params.contains("key2", "value2"));
    assert!(!search_params.contains_key("key3"));
    assert!(!search_params.contains("key1", "value2"));
    assert!(!search_params.contains("key3", "value3"));
}

#[test]
fn iterators() {
    let mut search_params =
        UrlSearchParams::parse("key1=value1&key1=value2&key2=value3").expect("parse");

    {
        let mut keys = search_params.keys();
        assert_eq!(keys.next(), Some("key1"));
        assert_eq!(keys.next(), Some("key1"));
        assert_eq!(keys.next(), Some("key2"));
        assert_eq!(keys.next(), None);
    }

    {
        let mut values = search_params.values();
        assert_eq!(values.next(), Some("value1"));
        assert_eq!(values.next(), Some("value2"));
        assert_eq!(values.next(), Some("value3"));
        assert_eq!(values.next(), None);
    }

    let mut entries = search_params.entries();
    assert_eq!(entries.next(), Some(("key1", "value1")));
    assert_eq!(entries.next(), Some(("key1", "value2")));
    assert_eq!(entries.next(), Some(("key2", "value3")));
    assert_eq!(entries.next(), None);
    drop(entries);

    search_params.append("foo", "bar");
    let mut appended_entries = search_params.entries();
    assert_eq!(appended_entries.next(), Some(("key1", "value1")));
    assert_eq!(appended_entries.next(), Some(("key1", "value2")));
    assert_eq!(appended_entries.next(), Some(("key2", "value3")));
    assert_eq!(appended_entries.next(), Some(("foo", "bar")));
    assert_eq!(appended_entries.next(), None);

    let mut expected = vec![
        ("foo", "bar"),
        ("key2", "value3"),
        ("key1", "value2"),
        ("key1", "value1"),
    ];
    for entry in search_params.entries() {
        let check = expected.pop().expect("expected element");
        assert_eq!(check, entry);
    }
    assert!(expected.is_empty());
}

#[test]
fn test_to_string_encoding() {
    let search_params =
        UrlSearchParams::parse("q1=foo&q2=foo+bar&q3=foo bar&q4=foo/bar").expect("parse");
    assert_eq!(search_params.get("q1"), Some("foo"));
    assert_eq!(search_params.get("q2"), Some("foo bar"));
    assert_eq!(search_params.get("q3"), Some("foo bar"));
    assert_eq!(search_params.get("q4"), Some("foo/bar"));
    assert_eq!(
        search_params.to_string(),
        "q1=foo&q2=foo+bar&q3=foo+bar&q4=foo%2Fbar"
    );
}

#[test]
fn test_character_set() {
    let mut search_params = UrlSearchParams::parse("key=value").expect("parse");
    let unique_keys = [
        '/', ':', ';', '=', '@', '[', ']', '^', '|', '$', '&', '+', ',', '!', '\'', ')', '~', '\\',
    ];
    for unique_key in unique_keys {
        let value = format!("value{unique_key}");
        search_params.set("key", &value);
        assert_eq!(search_params.get("key"), Some(value.as_str()));
        assert_ne!(search_params.to_string(), format!("key={value}"));
    }
}

#[test]
fn sort_unicode_code_units() {
    let mut search_params = UrlSearchParams::parse("ﬃ&🌈").expect("parse");
    search_params.sort();
    assert_eq!(search_params.len(), 2);
    let keys: Vec<_> = search_params.keys().collect();
    assert_eq!(keys, vec!["🌈", "ﬃ"]);
}

#[test]
fn sort_unicode_code_units_edge_case() {
    let mut search_params = UrlSearchParams::parse("🌈ﬃ&🌈").expect("parse");
    search_params.sort();
    assert_eq!(search_params.len(), 2);
    let keys: Vec<_> = search_params.keys().collect();
    assert_eq!(keys, vec!["🌈", "🌈ﬃ"]);
}
