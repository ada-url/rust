//! URL component serializers for IPv4 and IPv6.

#[cfg(not(feature = "std"))]
extern crate alloc;
#[cfg(feature = "std")]
extern crate std;
#[cfg(not(feature = "std"))]
use alloc::string::String;
#[cfg(feature = "std")]
use std::string::String;

/// Serialize a packed 32-bit IPv4 address to dotted-decimal notation.
pub fn ipv4(address: u64) -> String {
    let a = (address >> 24) as u8;
    let b = (address >> 16) as u8;
    let c = (address >> 8) as u8;
    let d = address as u8;
    format_ipv4(a, b, c, d)
}

#[cfg(feature = "std")]
fn format_ipv4(a: u8, b: u8, c: u8, d: u8) -> String {
    std::format!("{}.{}.{}.{}", a, b, c, d)
}

#[cfg(not(feature = "std"))]
fn format_ipv4(a: u8, b: u8, c: u8, d: u8) -> String {
    use core::fmt::Write;
    let mut s = String::new();
    let _ = write!(s, "{}.{}.{}.{}", a, b, c, d);
    s
}

/// Serialize an IPv6 address (8 x u16 pieces) to bracketed hex notation,
/// with the longest run of zeros compressed to "::".
pub fn ipv6(address: &[u16; 8]) -> String {
    // Find the longest run of zeros for compression
    let (compress, compress_len) = find_longest_zeros(address);

    let mut out = String::with_capacity(41); // max "[xxxx:xxxx:xxxx:xxxx:xxxx:xxxx:xxxx:xxxx]"
    out.push('[');

    let mut i = 0usize;
    while i < 8 {
        if compress_len > 1 && i == compress {
            out.push(':');
            if i == 0 {
                out.push(':');
            }
            i += compress_len;
            if i >= 8 {
                break;
            }
        }
        push_hex16(&mut out, address[i]);
        i += 1;
        if i < 8 {
            out.push(':');
        }
    }

    out.push(']');
    out
}

fn find_longest_zeros(addr: &[u16; 8]) -> (usize, usize) {
    let mut best_start = 0;
    let mut best_len = 0usize;
    let mut cur_start = 0;
    let mut cur_len = 0usize;
    for (i, &v) in addr.iter().enumerate() {
        if v == 0 {
            if cur_len == 0 {
                cur_start = i;
            }
            cur_len += 1;
            if cur_len > best_len {
                best_len = cur_len;
                best_start = cur_start;
            }
        } else {
            cur_len = 0;
        }
    }
    if best_len <= 1 {
        (8, 0) // no compression
    } else {
        (best_start, best_len)
    }
}

fn push_hex16(s: &mut String, val: u16) {
    // Output lowercase hex without leading zeros
    const HEX: &[u8] = b"0123456789abcdef";
    if val == 0 {
        s.push('0');
        return;
    }
    let mut buf = [0u8; 4];
    let mut n = 0;
    let mut v = val;
    while v > 0 {
        buf[n] = HEX[(v & 0xf) as usize];
        n += 1;
        v >>= 4;
    }
    // buf has the digits in reverse order
    for k in (0..n).rev() {
        s.push(buf[k] as char);
    }
}
