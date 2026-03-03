#![cfg(feature = "std")]

#[test]
#[ignore = "href_from_file is not exposed by the Rust public API"]
fn from_file_tests_basics() {}

#[test]
#[ignore = "user choice: keep IdnaTestV2 large case ignored"]
fn wpt_url_tests_idna_test_v2_to_ascii() {}

#[test]
#[ignore = "percent_encode internals are not exposed by the Rust public API"]
fn wpt_url_tests_percent_encoding() {}

#[test]
#[ignore = "has_valid_domain is not exposed by the Rust public API"]
fn wpt_url_tests_verify_dns_length() {}

#[test]
#[ignore = "URLPattern is not exposed by the Rust public API"]
fn basic_tests_test_workerd_issue_5144_4() {}

#[test]
#[ignore = "URLPattern suite is out of current wrapper scope"]
fn wpt_urlpattern_tests_all() {}

#[test]
#[ignore = "ada_search_params_reset is not exposed by the Rust public API"]
fn ada_c_ada_search_params_reset() {}

#[test]
#[ignore = "ada_get_version APIs are not exposed by the Rust public API"]
fn ada_c_ada_get_version() {}
