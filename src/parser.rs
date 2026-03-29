//! WHATWG URL parser state machine.
//! <https://url.spec.whatwg.org/#concept-url-parser>

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
use crate::scheme::{SchemeType as Scheme, get_scheme_type_lower};
use crate::unicode::{contains_xn_prefix_pub, is_alnum_plus, percent_encode};

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
        && !b.is_valid
    {
        return None;
    }

    // ── Fast path ─────────────────────────────────────────────────────────
    // Try the ultra-fast single-scan builder unconditionally — even when a
    // base is present.  An absolute URL (one that has its own scheme + "://")
    // never needs the base for resolution, so the base can be ignored when the
    // fast path succeeds.  The fast path is conservative: it returns None for
    // anything that could be relative or that requires the base (opaque paths,
    // same-scheme inputs without "//", relative paths, etc.), so falling
    // through to the state machine remains fully correct for those cases.
    if let Some(url) = try_parse_absolute_fast(user_input) {
        return Some(url);
    }

    // ── Relative URL fast paths ────────────────────────────────────────────
    // For the most common relative URL patterns (#fragment, ?query) we can
    // skip the full state machine entirely: C0-trim the input, then clone the
    // base URL and splice in only the changed components.  This avoids
    // `strip_tabs_newlines` + `trim_c0_whitespace` + all the state transitions
    // and per-component string copies the state machine would perform.
    if let Some(base_url) = base
        && let Some(url) = try_parse_relative_fast(user_input, base_url)
    {
        return Some(url);
    }

    // ── Slow path: full pre-processing + state machine ────────────────────
    let stripped = strip_tabs_newlines(user_input);
    let trimmed = trim_c0_whitespace(&stripped);
    let (url_data, fragment): (&str, Option<&str>) = match trimmed.find('#') {
        None => (trimmed, None),
        Some(p) => (&trimmed[..p], Some(&trimmed[p + 1..])),
    };

    let input_size = url_data.len();
    let b = url_data.as_bytes();

    let mut url = Url::empty();
    // When a base is present the output URL will typically include the base's
    // scheme + authority + path, so pre-reserve enough capacity to avoid
    // any realloc mid-construction.
    let initial_cap = match base {
        Some(base_url) => base_url.buffer.len() + input_size + 16,
        None => input_size + 4,
    };
    url.buffer.reserve(initial_cap);

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
                    if base.has_search() {
                        let s = base.search();
                        url.update_base_search(if s.is_empty() { "?" } else { s });
                    }
                    if pos < input_size && b[pos] == b'?' {
                        // Replace with fresh query; keep authority+path from base.
                        url.update_base_pathname(base.pathname());
                        url.clear_search();
                        state = State::Query;
                    } else if pos < input_size {
                        // Relative path resolution: copy the shortened base pathname.
                        //
                        // For the common case (base pathname does NOT start with "//"
                        // and scheme is not file:), we compute the shortened form
                        // directly — no temporary String allocation needed.
                        //
                        // When base.pathname() starts with "//" we must use the
                        // original two-step approach, because the first call triggers
                        // "/."-insertion side-effects that the second call relies on.
                        let base_path = base.pathname();
                        if !base_path.starts_with("//") && url.scheme != Scheme::File {
                            // Optimised single-call path — common case.
                            let cut = base_path.rfind('/').unwrap_or(base_path.len());
                            url.update_base_pathname(&base_path[..cut]);
                        } else {
                            // Original two-step — needed for "//"-prefixed paths and
                            // file: Windows-drive-letter edge cases.
                            url.update_base_pathname(base_path);
                            let mut tmp = url.pathname().to_string();
                            shorten_path(&mut tmp, url.scheme);
                            url.update_base_pathname(&tmp);
                        }
                        url.clear_search();
                        state = State::Path;
                        continue; // re-enter Path without advancing
                    } else {
                        url.update_base_pathname(base.pathname());
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
                        && base.scheme == Scheme::File
                    {
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
                let end = view.find(['/', '\\', '?']).unwrap_or(view.len());
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
                                // Shorten base pathname without allocating.
                                // Mirrors shorten_path() for file: scheme:
                                // if the path has no '/' after the first char
                                // AND looks like a drive letter, don't shorten.
                                let full = base.pathname();
                                let cut = if !full.is_empty()
                                    && full[1..].find('/').is_none()
                                    && is_normalized_windows_drive_letter(&full[1..])
                                {
                                    full.len() // drive-letter only — don't shorten
                                } else {
                                    full.rfind('/').unwrap_or(0)
                                };
                                url.update_base_pathname(&full[..cut]);
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

// =============================================================================
// Relative URL fast paths
// =============================================================================

/// Fast handler for the two most common relative URL patterns:
///
/// - `"#fragment"` → clone base, update fragment only
/// - `"?query[#fragment]"` → clone base, replace query (and optional fragment)
///
/// Returns `None` for anything that needs the full state machine (opaque base,
/// file: scheme, embedded tabs/newlines, other relative patterns, etc.).
fn try_parse_relative_fast(user_input: &str, base: &Url) -> Option<Url> {
    let raw = user_input.as_bytes();
    if raw.is_empty() {
        return None;
    }

    // ── Ultra-cheap first-byte gate ───────────────────────────────────────
    // For the majority of inputs that are NOT '#' or '?' we bail here with
    // minimal work: one byte read + two comparisons + zero heap traffic.
    // Leading C0 whitespace is rare; handle it only when necessary.
    let first = if raw[0] > b' ' {
        raw[0]
    } else {
        raw[raw.iter().position(|&b| b > b' ')?]
    };
    if first != b'#' && first != b'?' && first != b'/' {
        return None;
    }

    // ── Validate base ─────────────────────────────────────────────────────
    // Only handles non-opaque, non-file bases.
    if !base.is_valid || base.has_opaque_path || base.scheme == Scheme::File {
        return None;
    }

    // Full C0 trim + tab/newline check (only reached for '#' / '?' inputs)
    let start = if raw[0] <= b' ' {
        raw.iter().position(|&b| b > b' ')?
    } else {
        0
    };
    let end = if raw[raw.len() - 1] <= b' ' {
        raw.iter().rposition(|&b| b > b' ')? + 1
    } else {
        raw.len()
    };
    if start >= end {
        return None;
    }
    let t = &raw[start..end];

    // Bail on embedded tabs/newlines — state machine handles those
    if t.iter().any(|&b| matches!(b, b'\t' | b'\n' | b'\r')) {
        return None;
    }

    match t[0] {
        // ── "#fragment" ───────────────────────────────────────────────────────
        // Clone the base and update only the fragment.
        b'#' => {
            let frag = unsafe { core::str::from_utf8_unchecked(&t[1..]) };
            let mut url = base.clone();
            url.update_unencoded_base_hash(frag);
            Some(url)
        }

        // ── "?query[#fragment]" ───────────────────────────────────────────────
        // Clone the base, replace the query (percent-encoded per scheme), and
        // optionally set a fragment.
        b'?' => {
            // Split query from fragment at '#'
            let (query_bytes, frag_opt) = match t[1..].iter().position(|&b| b == b'#') {
                Some(p) => (&t[1..1 + p], Some(&t[2 + p..])),
                None => (&t[1..], None),
            };

            let mut url = base.clone();
            url.clear_hash();

            let query_str = unsafe { core::str::from_utf8_unchecked(query_bytes) };
            let encode_set = if url.is_special() {
                &crate::character_sets::SPECIAL_QUERY_PERCENT_ENCODE
            } else {
                &crate::character_sets::QUERY_PERCENT_ENCODE
            };
            url.update_base_search_with_encode(query_str, encode_set);

            if let Some(frag) = frag_opt {
                let frag_str = unsafe { core::str::from_utf8_unchecked(frag) };
                url.update_unencoded_base_hash(frag_str);
            }
            Some(url)
        }

        // ── "/abs-path[?query][#fragment]" ───────────────────────────────────
        // Clone the base authority (scheme + host + port), replace the path,
        // and optionally set a query and fragment.
        // Bail if the path needs encoding, contains a backslash, or has
        // dot-segments — the state machine handles those correctly.
        b'/' => {
            // Reject "//" (authority reference) — let state machine handle it
            if t.len() >= 2 && t[1] == b'/' {
                return None;
            }
            // Only handles non-opaque, non-file special bases
            if base.scheme == Scheme::File || !base.scheme.is_special() {
                return None;
            }

            // Split path / query / fragment
            let (path_bytes, rest) = match t.iter().position(|&b| b == b'?' || b == b'#') {
                Some(p) => (&t[..p], &t[p..]),
                None => (t, b"" as &[u8]),
            };
            let (query_opt, frag_opt) = if !rest.is_empty() {
                if rest[0] == b'?' {
                    match rest[1..].iter().position(|&b| b == b'#') {
                        Some(p) => (Some(&rest[1..1 + p]), Some(&rest[2 + p..])),
                        None => (Some(&rest[1..]), None),
                    }
                } else {
                    (None, Some(&rest[1..]))
                }
            } else {
                (None, None)
            };

            // Reject if path needs encoding, has backslash, or has dot-segments
            let path_str = unsafe { core::str::from_utf8_unchecked(path_bytes) };
            let sig = crate::checkers::path_signature(path_str);
            if sig & 0x0B != 0 {
                return None; // needs encoding / backslash / percent
            }
            if sig & 0x04 != 0 && path_str.contains("/.") {
                return None; // actual dot-segment present
            }

            // Clone base up to (but not including) pathname; set new path.
            let mut url = base.clone();
            url.clear_hash();
            url.clear_search();
            url.update_base_pathname(path_str);

            if let Some(q) = query_opt {
                let q_str = unsafe { core::str::from_utf8_unchecked(q) };
                let encode_set = if url.is_special() {
                    &crate::character_sets::SPECIAL_QUERY_PERCENT_ENCODE
                } else {
                    &crate::character_sets::QUERY_PERCENT_ENCODE
                };
                url.update_base_search_with_encode(q_str, encode_set);
            }
            if let Some(f) = frag_opt {
                url.update_unencoded_base_hash(unsafe { core::str::from_utf8_unchecked(f) });
            }
            Some(url)
        }

        _ => None,
    }
}

// =============================================================================
// Fast-path builder for absolute special URLs
// =============================================================================
//
// Handles the overwhelmingly common case: an absolute URL like
//   `https://hostname/path?query`
// with no base, no credentials, ASCII-only lowercase host, and no path dots.
//
// Single forward scan replaces the full state-machine loop.  Falls back to
// `None` for anything unusual so the slow path covers all edge cases.

/// Try to parse `url_data` (already C0-trimmed, tab/newline-free, fragment-stripped)
/// as a simple absolute special URL without invoking the state machine.
/// Fast-path builder for absolute special URLs — takes RAW (unprocessed) input.
///
/// Inlines C0 trim, tab/newline detection, fragment split, host validation,
/// and buffer construction into a minimal number of forward scans.
/// Falls back to `None` (→ state machine) for anything unusual.
#[inline]
pub(crate) fn try_parse_absolute_fast(raw_input: &str) -> Option<Url> {
    use crate::character_sets::FRAGMENT_PERCENT_ENCODE;
    use crate::unicode::DOMAIN_CHECK;
    use crate::{HostKind, OMITTED};

    let raw = raw_input.as_bytes();

    // ── C0 whitespace trim (branchless for typical URLs with none) ─────────
    let start = if !raw.is_empty() && raw[0] <= b' ' {
        raw.iter().position(|&b| b > b' ')?
    } else {
        0
    };
    let end = if !raw.is_empty() && raw[raw.len() - 1] <= b' ' {
        raw.iter().rposition(|&b| b > b' ').map(|i| i + 1)?
    } else {
        raw.len()
    };
    if start >= end {
        return None;
    }
    let b = &raw[start..end];

    // ── Scheme detection (≤ 6 bytes) ──────────────────────────────────────
    if b.is_empty() || !is_alpha(b[0]) {
        return None;
    }

    let colon = {
        let mut i = 1usize;
        loop {
            if i >= b.len().min(7) {
                return None;
            }
            match b[i] {
                b':' => break i,
                c if !is_alnum_plus(c) => return None, // invalid scheme char (incl. \t\n\r)
                _ => i += 1,
            }
        }
    };

    // Perfect-hash scheme type (no string comparison)
    let scheme_bytes = &b[..colon];
    let scheme = {
        let s = unsafe { core::str::from_utf8_unchecked(scheme_bytes) };
        let t = get_scheme_type_lower(s);
        if t == Scheme::NotSpecial {
            crate::scheme::get_scheme_type(s)
        } else {
            t
        }
    };
    if !scheme.is_special() || scheme == Scheme::File {
        return None;
    }

    // ── Require "://" ──────────────────────────────────────────────────────
    if b.len() < colon + 3 || b[colon + 1] != b'/' || b[colon + 2] != b'/' {
        return None;
    }
    let auth_start = colon + 3;

    // ── Single-pass authority scan ─────────────────────────────────────────
    // Simultaneously: find auth end, detect '@' / tabs / non-ASCII / uppercase,
    // IPv4 flag (only digits+dots), xn-- flag ('x' present), forbidden chars.
    let mut auth_end = auth_start;
    let mut port_colon: Option<usize> = None;
    let mut has_x = false; // xn-- candidate ('x' seen in host)

    while auth_end < b.len() {
        let c = b[auth_end];
        match c {
            b'/' | b'?' | b'#' | b'\\' => break,
            b'@' => return None,
            b':' if port_colon.is_none() => {
                port_colon = Some(auth_end);
                auth_end += 1;
            }
            b'\t' | b'\n' | b'\r' => return None,
            c if c >= 0x80 => return None,
            b'0'..=b'9' | b'.' => {
                auth_end += 1;
            }
            c => {
                if c == b'x' {
                    has_x = true;
                }
                if DOMAIN_CHECK[c as usize] != 0 {
                    return None;
                }
                auth_end += 1;
            }
        }
    }

    let host_end_in_input = port_colon.unwrap_or(auth_end);
    let host = &b[auth_start..host_end_in_input];
    if host.is_empty() {
        return None;
    }

    // IPv4 quick-filter: check the last *significant* (non-dot) byte of the host.
    // For TLD hostnames (.com/.org/.net) the last letter is 'm','g','t' — never
    // in {0-9, a-f, x} — so the IPv4 path is never entered for typical domains.
    let mut ipv4_val: Option<u32> = None;
    {
        let last_sig = host
            .iter()
            .rev()
            .find(|&&c| c != b'.')
            .copied()
            .unwrap_or(0);
        let maybe_ipv4 =
            last_sig.is_ascii_digit() || matches!(last_sig, b'a'..=b'f') || last_sig == b'x';
        if maybe_ipv4 {
            let host_str = unsafe { core::str::from_utf8_unchecked(host) };
            if crate::checkers::is_ipv4(host_str) {
                // Try the fast pure-decimal path first (avoids the full parser for
                // the most common form — "192.168.1.1" etc.)
                let fast = crate::checkers::try_parse_ipv4_fast(host_str);
                if fast != u64::MAX {
                    ipv4_val = Some(fast as u32);
                } else {
                    // Non-decimal or non-four-part (e.g. "0x7f.1", "0xc0a80101"):
                    // full WHATWG parser handles hex/octal/fewer parts.
                    match crate::checkers::parse_ipv4_address(host_str) {
                        Some(v) => ipv4_val = Some(v),
                        None => return None, // malformed — let slow path reject properly
                    }
                }
            }
        }
    }

    // xn-- check: only if 'x' was seen (zero cost for typical .com/.org hosts)
    if has_x {
        let host_str = unsafe { core::str::from_utf8_unchecked(host) };
        if contains_xn_prefix_pub(host_str) {
            return None;
        }
    }

    // ── Port ────────────────────────────────────────────────────────────────
    let port_val: u32 = if let Some(pc) = port_colon {
        let port_bytes = &b[pc + 1..auth_end];
        if port_bytes.is_empty() {
            OMITTED
        } else {
            // Port must be ≤ 5 digits (max valid port 65535); longer strings
            // would overflow a u32 in the fold below.
            if port_bytes.len() > 5 || !port_bytes.iter().all(|&c| c.is_ascii_digit()) {
                return None;
            }
            let n: u32 = port_bytes
                .iter()
                .fold(0u32, |a, &c| a * 10 + (c - b'0') as u32);
            if n > 65535 {
                return None;
            }
            let def = scheme.default_port();
            if def != 0 && n as u16 == def {
                OMITTED
            } else {
                n
            }
        }
    } else {
        OMITTED
    };

    // ── Path + query + fragment scan ───────────────────────────────────────
    let path_start = auth_end;
    let mut query_start: Option<usize> = None;
    let mut frag_start: Option<usize> = None;
    let path_end: usize;

    {
        let mut i = path_start;
        loop {
            if i >= b.len() {
                path_end = i;
                break;
            }
            match b[i] {
                b'?' => {
                    path_end = i;
                    query_start = Some(i);
                    break;
                }
                b'#' => {
                    path_end = i;
                    frag_start = Some(i);
                    break;
                }
                b'\\' => return None, // backslash needs normalisation
                _ => {}
            }
            i += 1;
        }
    }

    // Path: use path_signature to detect encoding needs + dot-segments.
    let path_bytes = &b[path_start..path_end];
    let path_sig =
        crate::checkers::path_signature(unsafe { core::str::from_utf8_unchecked(path_bytes) });
    if path_sig & 0x0B != 0 {
        return None;
    } // needs encoding / backslash / percent
    if path_sig & 0x04 != 0 {
        // Has a dot — check for actual dot-segments (SIMD str::contains)
        let path_str = unsafe { core::str::from_utf8_unchecked(path_bytes) };
        if path_str.contains("/.") {
            return None;
        }
    }

    // Query: check for characters needing encoding
    let query_end = frag_start.unwrap_or(b.len());
    if let Some(qs) = query_start {
        let qbytes = &b[qs + 1..query_end];
        let encode_set = if scheme.default_port() != 0 {
            &crate::character_sets::SPECIAL_QUERY_PERCENT_ENCODE
        } else {
            &crate::character_sets::QUERY_PERCENT_ENCODE
        };
        if crate::unicode::percent_encode_index(
            unsafe { core::str::from_utf8_unchecked(qbytes) },
            encode_set,
        ) != qbytes.len()
        {
            return None; // query needs encoding
        }
    }

    // Fragment from '#' in the URL (may be None if no '#')
    let fragment: Option<&str> =
        frag_start.map(|fs| unsafe { core::str::from_utf8_unchecked(&b[fs + 1..]) });

    // ── Build URL buffer in a single forward write pass ────────────────────
    // For IPv4 hosts the canonical form is at most 15 chars ("255.255.255.255"),
    // which may differ from the raw input (e.g. "0x7f.1" → "127.0.0.1").
    let host_len_est = if ipv4_val.is_some() { 15 } else { host.len() };
    let total = colon + 1 + 2 // scheme://
        + host_len_est
        + if port_val != OMITTED { 6 } else { 0 }
        + (path_end - path_start).max(1)
        + query_start.map_or(0, |qs| query_end - qs)
        + fragment.map_or(0, |f| f.len() + 1);

    let mut url = Url::empty();
    url.scheme = scheme;
    url.buffer.reserve(total + 4);

    // Scheme (lowercase) + ':'
    for &c in scheme_bytes {
        url.buffer.push((c | 0x20) as char);
    }
    url.buffer.push(':');
    url.components.protocol_end = url.buffer.len() as u32;

    // "//"
    url.buffer.push('/');
    url.buffer.push('/');
    url.components.username_end = url.buffer.len() as u32;
    url.components.host_start = url.buffer.len() as u32;

    // Host — normalise IPv4 to dotted-decimal, or write domain as-is
    if let Some(v4) = ipv4_val {
        crate::serializers::write_ipv4(&mut url.buffer, v4);
        url.host_kind = HostKind::Ipv4;
    } else {
        url.buffer
            .push_str(unsafe { core::str::from_utf8_unchecked(host) });
        url.host_kind = HostKind::Domain;
    }
    url.components.host_end = url.buffer.len() as u32;

    // Port
    if port_val != OMITTED {
        url.buffer.push(':');
        let mut tmp = [0u8; 5];
        let mut n = port_val;
        let mut len = 0usize;
        loop {
            tmp[len] = b'0' + (n % 10) as u8;
            n /= 10;
            len += 1;
            if n == 0 {
                break;
            }
        }
        for k in (0..len).rev() {
            url.buffer.push(tmp[k] as char);
        }
        url.components.port = port_val;
    }
    url.components.pathname_start = url.buffer.len() as u32;

    // Path
    if path_bytes.is_empty() {
        url.buffer.push('/');
    } else {
        url.buffer
            .push_str(unsafe { core::str::from_utf8_unchecked(path_bytes) });
    }

    // Query (with leading '?')
    if let Some(qs) = query_start {
        url.components.search_start = url.buffer.len() as u32;
        url.buffer
            .push_str(unsafe { core::str::from_utf8_unchecked(&b[qs..query_end]) });
    }

    // Fragment (percent-encoded, with leading '#')
    if let Some(frag) = fragment {
        url.components.hash_start = url.buffer.len() as u32;
        url.buffer.push('#');
        let enc = percent_encode(frag, &FRAGMENT_PERCENT_ENCODE);
        url.buffer.push_str(&enc);
    }

    Some(url)
}

// ============================================================
// Zero-allocation URL validator for can_parse
// ============================================================

/// Validate an absolute URL in one forward scan — **zero heap allocations**.
///
/// Unlike `try_parse_absolute_fast` this:
///  - builds no buffer (no `String` allocation at all)
///  - skips the query/path encoding check (any query/path is structurally valid —
///    the parser would just encode whatever is there)
///  - returns `None` rather than `false` for credentials (`@`) and IPv4 so that
///    the full validator can handle those edge cases
///
/// Returns `Some(())` = definitely valid, `None` = fall back to full validator.
#[inline]
pub(crate) fn try_validate_absolute_fast(raw_input: &str) -> Option<()> {
    use crate::unicode::{DOMAIN_CHECK, contains_xn_prefix_pub};

    let raw = raw_input.as_bytes();

    // C0 trim — O(1) for typical URLs
    let start = if !raw.is_empty() && raw[0] <= b' ' {
        raw.iter().position(|&b| b > b' ')?
    } else {
        0
    };
    let end = if !raw.is_empty() && raw[raw.len() - 1] <= b' ' {
        raw.iter().rposition(|&b| b > b' ').map(|i| i + 1)?
    } else {
        raw.len()
    };
    if start >= end {
        return None;
    }
    let b = &raw[start..end];

    // Scheme (≤ 6 significant bytes before ':')
    if b.is_empty() || !is_alpha(b[0]) {
        return None;
    }
    let colon = {
        let mut i = 1usize;
        loop {
            if i >= b.len().min(7) {
                return None;
            }
            match b[i] {
                b':' => break i,
                c if !is_alnum_plus(c) => return None, // also catches \t\n\r
                _ => i += 1,
            }
        }
    };

    // Perfect-hash scheme classification (no string comparison)
    let scheme = {
        let s = unsafe { core::str::from_utf8_unchecked(&b[..colon]) };
        let t = get_scheme_type_lower(s);
        if t == Scheme::NotSpecial {
            crate::scheme::get_scheme_type(s)
        } else {
            t
        }
    };
    if !scheme.is_special() || scheme == Scheme::File {
        return None;
    }

    // Require "://"
    if b.len() < colon + 3 || b[colon + 1] != b'/' || b[colon + 2] != b'/' {
        return None;
    }
    let auth_start = colon + 3;

    // Single-pass authority scan
    let mut auth_end = auth_start;
    let mut port_colon: Option<usize> = None;
    let mut has_x = false;

    while auth_end < b.len() {
        let c = b[auth_end];
        match c {
            b'/' | b'?' | b'#' | b'\\' => break,
            // @ means credentials — not a hard failure, but we can't validate
            // the user/password portion cheaply; defer to the full validator
            b'@' => return None,
            b':' if port_colon.is_none() => {
                port_colon = Some(auth_end);
                auth_end += 1;
            }
            b'\t' | b'\n' | b'\r' => return None,
            c if c >= 0x80 => return None,
            b'0'..=b'9' | b'.' => auth_end += 1,
            c => {
                if c == b'x' {
                    has_x = true;
                }
                if DOMAIN_CHECK[c as usize] != 0 {
                    return None;
                }
                auth_end += 1;
            }
        }
    }

    let host_end_in_input = port_colon.unwrap_or(auth_end);
    let host = &b[auth_start..host_end_in_input];
    if host.is_empty() {
        return None;
    }

    // IPv4 quick filter: last significant (non-dot) byte in {0-9, a-f, x}
    // → might be an IPv4 address, let the full validator decide
    let last_sig = host
        .iter()
        .rev()
        .find(|&&c| c != b'.')
        .copied()
        .unwrap_or(0);
    if last_sig.is_ascii_digit() || matches!(last_sig, b'a'..=b'f') || last_sig == b'x' {
        return None; // possibly IPv4 → full validator handles it
    }

    // xn-- check (only if 'x' was seen — skipped for typical .com/.org domains)
    if has_x {
        let host_str = unsafe { core::str::from_utf8_unchecked(host) };
        if contains_xn_prefix_pub(host_str) {
            return None;
        }
    }

    // Port validation
    if let Some(pc) = port_colon {
        let port_bytes = &b[pc + 1..auth_end];
        if !port_bytes.is_empty() {
            if port_bytes.len() > 5 || !port_bytes.iter().all(|&c| c.is_ascii_digit()) {
                return None;
            }
            let n: u32 = port_bytes
                .iter()
                .fold(0u32, |a, &c| a * 10 + (c - b'0') as u32);
            if n > 65535 {
                return None;
            }
        }
    }

    // Path, query, and fragment are always structurally valid for can_parse —
    // the parser encodes whatever is there.  No check needed.

    Some(())
}
