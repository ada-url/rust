//! WHATWG URL parser state machine.
//! https://url.spec.whatwg.org/#concept-url-parser

#[cfg(not(feature = "std"))]
extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

#[cfg(not(feature = "std"))]
use alloc::string::ToString;

use crate::Url;
use crate::character_sets::{
    C0_CONTROL_PERCENT_ENCODE, QUERY_PERCENT_ENCODE, SPECIAL_QUERY_PERCENT_ENCODE,
};
use crate::checkers::{is_alpha, is_normalized_windows_drive_letter, is_windows_drive_letter};
use crate::helpers::{
    find_authority_delimiter, find_authority_delimiter_special, get_host_delimiter_location,
    shorten_path, strip_tabs_newlines, trim_c0_whitespace,
};
use crate::scheme::SchemeType as Scheme;
use crate::unicode::{is_alnum_plus, percent_encode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    SchemeStart,
    Scheme,
    NoScheme,
    SpecialRelativeOrAuthority,
    PathOrAuthority,
    RelativeScheme,
    RelativeSlash,
    SpecialAuthoritySlashes,
    SpecialAuthorityIgnoreSlashes,
    Authority,
    Host,
    Port,
    PathStart,
    Path,
    File,
    FileSlash,
    FileHost,
    Query,
    OpaquePath,
}

/// Parse `user_input` relative to `base`. Returns `Some(Url)` on success.
pub fn parse_url(user_input: &str, base: Option<&Url>) -> Option<Url> {
    if user_input.len() > u32::MAX as usize {
        return None;
    }
    if let Some(b) = base
        && !b.is_valid {
            return None;
        }

    // Strip tabs/newlines — CoW: zero allocation when none present (common case)
    let stripped = strip_tabs_newlines(user_input);

    // Trim C0 whitespace — borrow from the (possibly borrowed) cow, no allocation
    let url_data = trim_c0_whitespace(&stripped);

    // Extract fragment — split in-place, no allocation (fragment borrows from url_data)
    let (url_data, fragment) = match url_data.find('#') {
        None    => (url_data, None),
        Some(p) => (&url_data[..p], Some(&url_data[p + 1..])),
    };

    let input_size = url_data.len();
    let b = url_data.as_bytes();

    let mut url = Url::empty();
    url.buffer.reserve(input_size + 16);

    let mut state = State::SchemeStart;
    let mut pos: usize = 0;

    // The main parsing loop.  We use `continue` to re-enter the current state
    // without advancing `pos`, and fall through naturally to advance by one.
    loop {
        match state {
            // ----------------------------------------------------------------
            State::SchemeStart => {
                if pos < input_size && is_alpha(b[pos]) {
                    state = State::Scheme;
                    pos += 1;
                } else {
                    state = State::NoScheme;
                }
            }
            // ----------------------------------------------------------------
            State::Scheme => {
                while pos < input_size && is_alnum_plus(b[pos]) {
                    pos += 1;
                }
                if pos < input_size && b[pos] == b':' {
                    if !url.parse_scheme_with_colon(&url_data[..pos + 1]) {
                        return None;
                    }
                    pos += 1;
                    state = match url.scheme {
                        Scheme::File => State::File,
                        s if s.is_special() && base.is_some_and(|b| b.scheme == s) => {
                            State::SpecialRelativeOrAuthority
                        }
                        s if s.is_special() => State::SpecialAuthoritySlashes,
                        _ if pos < input_size && b[pos] == b'/' => {
                            pos += 1;
                            State::PathOrAuthority
                        }
                        _ => State::OpaquePath,
                    };
                } else {
                    state = State::NoScheme;
                    pos = 0;
                    continue;
                }
            }
            // ----------------------------------------------------------------
            State::NoScheme => {
                let base = match base {
                    None => {
                        url.is_valid = false;
                        return None;
                    }
                    Some(b) => b,
                };
                if base.has_opaque_path && fragment.is_none() {
                    url.is_valid = false;
                    return None;
                }
                if base.has_opaque_path && fragment.is_some() && pos == input_size {
                    url.copy_scheme(base);
                    url.has_opaque_path = base.has_opaque_path;
                    url.update_base_pathname(base.pathname());
                    if base.has_search() {
                        let s = base.search();
                        url.update_base_search(if s.is_empty() { "?" } else { s });
                    }
                    if let Some(frag) = fragment {
                        url.update_unencoded_base_hash(frag);
                    }
                    return Some(url);
                }
                state = if base.scheme != Scheme::File {
                    State::RelativeScheme
                } else {
                    State::File
                };
            }
            // ----------------------------------------------------------------
            State::SpecialRelativeOrAuthority => {
                if url_data[pos..].starts_with("//") {
                    state = State::SpecialAuthorityIgnoreSlashes;
                    pos += 2;
                } else {
                    state = State::RelativeScheme;
                }
            }
            // ----------------------------------------------------------------
            State::PathOrAuthority => {
                if pos < input_size && b[pos] == b'/' {
                    state = State::Authority;
                    pos += 1;
                } else {
                    state = State::Path;
                }
            }
            // ----------------------------------------------------------------
            State::RelativeScheme => {
                let base = base.unwrap();
                url.copy_scheme(base);
                if pos < input_size && (b[pos] == b'/' || (url.is_special() && b[pos] == b'\\')) {
                    state = State::RelativeSlash;
                } else {
                    // Copy everything from base
                    url.update_base_authority(base.buffer.as_str(), &base.components);
                    url.update_host_to_base_host(base.hostname());
                    url.update_base_port(base.retrieve_base_port());
                    url.has_opaque_path = base.has_opaque_path;
                    url.update_base_pathname(base.pathname());
                    if base.has_search() {
                        let s = base.search();
                        url.update_base_search(if s.is_empty() { "?" } else { s });
                    }
                    if pos < input_size && b[pos] == b'?' {
                        url.clear_search();
                        state = State::Query;
                    } else if pos < input_size {
                        url.clear_search();
                        let mut path = url.pathname().to_string();
                        shorten_path(&mut path, url.scheme);
                        url.update_base_pathname(&path);
                        state = State::Path;
                        continue; // re-enter Path without advancing
                    }
                }
                pos += 1;
            }
            // ----------------------------------------------------------------
            State::RelativeSlash => {
                if url.is_special() && pos < input_size && (b[pos] == b'/' || b[pos] == b'\\') {
                    state = State::SpecialAuthorityIgnoreSlashes;
                } else if pos < input_size && b[pos] == b'/' {
                    state = State::Authority;
                } else {
                    let base = base.unwrap();
                    url.update_base_authority(base.buffer.as_str(), &base.components);
                    url.update_host_to_base_host(base.hostname());
                    url.update_base_port(base.retrieve_base_port());
                    state = State::Path;
                    continue;
                }
                pos += 1;
            }
            // ----------------------------------------------------------------
            State::SpecialAuthoritySlashes => {
                if url_data[pos..].starts_with("//") {
                    pos += 2;
                }
                state = State::SpecialAuthorityIgnoreSlashes;
                continue;
            }
            // ----------------------------------------------------------------
            State::SpecialAuthorityIgnoreSlashes => {
                while pos < input_size && (b[pos] == b'/' || b[pos] == b'\\') {
                    pos += 1;
                }
                state = State::Authority;
            }
            // ----------------------------------------------------------------
            State::Authority => {
                if !url_data[pos..].contains('@') {
                    state = State::Host;
                    continue;
                }
                let mut at_seen = false;
                let mut pw_seen = false;
                loop {
                    let view = &url_data[pos..];
                    let loc = if url.is_special() {
                        find_authority_delimiter_special(view)
                    } else {
                        find_authority_delimiter(view)
                    };
                    let end = pos + loc;

                    if end < input_size && b[end] == b'@' {
                        if at_seen {
                            if pw_seen {
                                url.append_base_password("%40");
                            } else {
                                url.append_base_username("%40");
                            }
                        }
                        at_seen = true;
                        let av = &url_data[pos..end];
                        if !pw_seen {
                            if let Some(cp) = av.find(':') {
                                pw_seen = true;
                                url.append_base_username(&percent_encode(
                                    &av[..cp],
                                    &crate::character_sets::USERINFO_PERCENT_ENCODE,
                                ));
                                url.append_base_password(&percent_encode(
                                    &av[cp + 1..],
                                    &crate::character_sets::USERINFO_PERCENT_ENCODE,
                                ));
                            } else {
                                url.append_base_username(&percent_encode(
                                    av,
                                    &crate::character_sets::USERINFO_PERCENT_ENCODE,
                                ));
                            }
                        } else {
                            url.append_base_password(&percent_encode(
                                av,
                                &crate::character_sets::USERINFO_PERCENT_ENCODE,
                            ));
                        }
                    } else if end == input_size
                        || b[end] == b'/'
                        || b[end] == b'?'
                        || (url.is_special() && b[end] == b'\\')
                    {
                        if at_seen && url_data[pos..end].is_empty() {
                            url.is_valid = false;
                            return None;
                        }
                        state = State::Host;
                        break;
                    }

                    if end == input_size {
                        if let Some(frag) = fragment {
                            url.update_unencoded_base_hash(frag);
                        }
                        return Some(url);
                    }
                    pos = end + 1;
                }
            }
            // ----------------------------------------------------------------
            State::Host => {
                let host_view = &url_data[pos..];
                let (loc, found_colon, trimmed) =
                    get_host_delimiter_location(url.is_special(), host_view);
                let new_pos = if loc < host_view.len() {
                    pos + loc
                } else {
                    input_size
                };
                pos = new_pos;
                if found_colon {
                    if !url.parse_host(trimmed) {
                        return None;
                    }
                    state = State::Port;
                    pos += 1;
                } else {
                    if trimmed.is_empty() && url.is_special() {
                        url.is_valid = false;
                        return None;
                    }
                    if trimmed.is_empty() {
                        url.update_base_hostname("");
                    } else if !url.parse_host(trimmed) {
                        return None;
                    }
                    state = State::PathStart;
                }
            }
            // ----------------------------------------------------------------
            State::Port => {
                let consumed = url.parse_port(&url_data[pos..], true);
                if !url.is_valid {
                    return None;
                }
                pos += consumed;
                state = State::PathStart;
                continue;
            }
            // ----------------------------------------------------------------
            State::PathStart => {
                if url.is_special() {
                    state = State::Path;
                    if pos == input_size {
                        url.update_base_pathname("/");
                        if let Some(frag) = fragment {
                            url.update_unencoded_base_hash(frag);
                        }
                        return Some(url);
                    }
                    if b[pos] != b'/' && b[pos] != b'\\' {
                        continue;
                    }
                } else if pos < input_size && b[pos] == b'?' {
                    state = State::Query;
                } else if pos < input_size {
                    state = State::Path;
                    if b[pos] != b'/' {
                        continue;
                    }
                } else {
                    if let Some(frag) = fragment {
                        url.update_unencoded_base_hash(frag);
                    }
                    return Some(url);
                }
                pos += 1;
            }
            // ----------------------------------------------------------------
            State::Path => {
                let view = &url_data[pos..];
                let (path_view, advance, done) = match view.find('?') {
                    Some(q) => {
                        state = State::Query;
                        (&view[..q], q + 1, false)
                    }
                    None => (view, view.len(), true),
                };
                url.consume_prepared_path(path_view);
                pos += advance;
                if done {
                    if let Some(frag) = fragment {
                        url.update_unencoded_base_hash(frag);
                    }
                    return Some(url);
                }
                continue;
            }
            // ----------------------------------------------------------------
            State::OpaquePath => {
                let view = &url_data[pos..];
                let (path_view, advance, done) = match view.find('?') {
                    Some(q) => {
                        state = State::Query;
                        (&view[..q], q + 1, false)
                    }
                    None => (view, view.len(), true),
                };
                url.has_opaque_path = true;
                let enc = if let Some(trimmed) = path_view.strip_suffix(' ') {
                    let mut s = percent_encode(trimmed, &C0_CONTROL_PERCENT_ENCODE).into_owned();
                    s.push_str("%20");
                    s
                } else {
                    percent_encode(path_view, &C0_CONTROL_PERCENT_ENCODE).into_owned()
                };
                url.update_base_pathname(&enc);
                pos += advance;
                if done {
                    if let Some(frag) = fragment {
                        url.update_unencoded_base_hash(frag);
                    }
                    return Some(url);
                }
                continue;
            }
            // ----------------------------------------------------------------
            State::Query => {
                let qv = &url_data[pos..];
                let set = if url.is_special() {
                    &SPECIAL_QUERY_PERCENT_ENCODE
                } else {
                    &QUERY_PERCENT_ENCODE
                };
                url.update_base_search_with_encode(qv, set);
                if let Some(frag) = fragment {
                    url.update_unencoded_base_hash(frag);
                }
                return Some(url);
            }
            // ----------------------------------------------------------------
            State::FileSlash => {
                if pos < input_size && (b[pos] == b'/' || b[pos] == b'\\') {
                    state = State::FileHost;
                    pos += 1;
                } else {
                    if let Some(base) = base
                        && base.scheme == Scheme::File {
                            url.update_host_to_base_host(base.host());
                            let bp = base.pathname();
                            if !bp.is_empty() && !is_windows_drive_letter(&url_data[pos..]) {
                                let first = &bp[1..];
                                let seg_end = first.find('/').unwrap_or(first.len());
                                let seg = &first[..seg_end];
                                if is_normalized_windows_drive_letter(seg) {
                                    let mut s = "/".to_string();
                                    s.push_str(seg);
                                    url.append_base_pathname(&s);
                                }
                            }
                        }
                    state = State::Path;
                    continue;
                }
            }
            // ----------------------------------------------------------------
            State::FileHost => {
                let view = &url_data[pos..];
                let end = view
                    .find(['/', '\\', '?'])
                    .unwrap_or(view.len());
                let fhb = &view[..end];
                if is_windows_drive_letter(fhb) {
                    state = State::Path;
                } else if fhb.is_empty() {
                    url.update_base_hostname("");
                    state = State::PathStart;
                } else {
                    pos += fhb.len();
                    if !url.parse_host(fhb) {
                        return None;
                    }
                    if url.hostname() == "localhost" {
                        url.update_base_hostname("");
                    }
                    state = State::PathStart;
                    continue;
                }
            }
            // ----------------------------------------------------------------
            State::File => {
                url.set_protocol_as_file();
                url.update_base_hostname("");
                if pos < input_size && (b[pos] == b'/' || b[pos] == b'\\') {
                    state = State::FileSlash;
                } else if let Some(base) = base {
                    if base.scheme == Scheme::File {
                        url.update_host_to_base_host(base.hostname());
                        url.update_base_pathname(base.pathname());
                        if base.has_search() {
                            let s = base.search();
                            url.update_base_search(if s.is_empty() { "?" } else { s });
                        }
                        url.has_opaque_path = base.has_opaque_path;
                        if pos < input_size && b[pos] == b'?' {
                            url.clear_search();
                            state = State::Query;
                            // fall through to pos += 1 below
                        } else if pos < input_size {
                            url.clear_search();
                            let fv = &url_data[pos..];
                            if !is_windows_drive_letter(fv) {
                                let mut path = url.pathname().to_string();
                                shorten_path(&mut path, url.scheme);
                                url.update_base_pathname(&path);
                            } else {
                                url.clear_pathname();
                                url.has_opaque_path = true;
                            }
                            state = State::Path;
                            continue; // "decrease pointer by 1"
                        } else {
                            // EOF: base was fully copied, nothing more to parse
                            if let Some(frag) = fragment {
                                url.update_unencoded_base_hash(frag);
                            }
                            return Some(url);
                        }
                    } else {
                        // base exists but is not a file URL
                        state = State::Path;
                        continue;
                    }
                } else {
                    state = State::Path;
                    continue;
                }
                pos += 1;
            }
        }

        if pos > input_size {
            break;
        }
    }

    if let Some(frag) = fragment {
        url.update_unencoded_base_hash(frag);
    }
    Some(url)
}
