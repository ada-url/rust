#![cfg(feature = "std")]

use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

pub fn fixture_path(relative: &str) -> PathBuf {
    let cwd_path = Path::new(relative);
    if cwd_path.exists() {
        return cwd_path.to_path_buf();
    }

    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}

pub fn read_fixture(relative: &str) -> String {
    let path = fixture_path(relative);
    fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()))
}

pub fn read_json(relative: &str) -> Value {
    let raw = read_fixture(relative);
    serde_json::from_str(&raw)
        .unwrap_or_else(|err| panic!("failed to parse JSON fixture {relative}: {err}"))
}

pub fn as_array<'a>(value: &'a Value, context: &str) -> &'a Vec<Value> {
    value
        .as_array()
        .unwrap_or_else(|| panic!("{context} is not a JSON array"))
}

pub fn as_object<'a>(value: &'a Value, context: &str) -> &'a serde_json::Map<String, Value> {
    value
        .as_object()
        .unwrap_or_else(|| panic!("{context} is not a JSON object"))
}

pub fn get_str<'a>(obj: &'a serde_json::Map<String, Value>, key: &str) -> &'a str {
    obj.get(key)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("missing string key `{key}`"))
}

pub fn maybe_str<'a>(obj: &'a serde_json::Map<String, Value>, key: &str) -> Option<&'a str> {
    obj.get(key).and_then(Value::as_str)
}
