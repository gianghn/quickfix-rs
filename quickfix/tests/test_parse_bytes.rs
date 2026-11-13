//! Phase 15: frames are parsed from bytes, so non-UTF-8 text and embedded NULs reach `QuickFIX`
//! as they are and come back byte-exact (the `&str` entry points cannot carry either).

use quickfix::*;
use utils::raw_peer::wire;

mod utils;

const FIX44: &str = "../quickfix-ffi/libquickfix/spec/FIX44.xml";

/// Header fields in the order `QuickFIX` writes them, so a parsed frame serializes back the same.
fn frame(msg_type: &'static [u8], body: &[(i32, &'static [u8])]) -> Vec<u8> {
    let mut fields = vec![
        (35, msg_type),
        (34, &b"2"[..]),
        (49, b"SENDER"),
        (52, b"20261111-02:15:00.000"),
        (56, b"RECEIVER"),
    ];
    fields.extend_from_slice(body);
    wire(&fields)
}

/// A Reject whose Text (58) is Latin-1 `café`: not UTF-8.
fn latin1_text() -> Vec<u8> {
    frame(b"3", &[(45, b"1"), (58, b"caf\xE9")])
}

/// A Logon whose raw data (96) holds a NUL.
fn nul_raw_data() -> Vec<u8> {
    frame(b"A", &[(95, b"3"), (96, b"a\0b"), (98, b"0"), (108, b"30")])
}

fn assert_round_trip(message: &Message, wire: &[u8], tag: i32, value: &[u8]) {
    assert_eq!(message.get_field_bytes(tag).as_deref(), Some(value));
    assert_eq!(message.to_fix_bytes().as_deref(), Ok(wire));
}

#[test]
fn try_from_bytes_returns_message_byte_exact_when_text_is_not_utf8() -> Result<(), QuickFixError> {
    let wire = latin1_text();
    assert!(std::str::from_utf8(&wire).is_err());
    assert_round_trip(&Message::try_from_bytes(&wire)?, &wire, 58, b"caf\xE9");
    Ok(())
}

#[test]
fn try_from_bytes_returns_message_byte_exact_when_data_holds_nul() -> Result<(), QuickFixError> {
    let wire = nul_raw_data();
    assert_round_trip(&Message::try_from_bytes(&wire)?, &wire, 96, b"a\0b");
    Ok(())
}

#[test]
fn try_build_message_bytes_returns_message_byte_exact_when_text_is_not_utf8(
) -> Result<(), QuickFixError> {
    let dd = DataDictionary::try_from_path(FIX44)?;
    let wire = latin1_text();
    assert_round_trip(&dd.try_build_message_bytes(&wire)?, &wire, 58, b"caf\xE9");
    Ok(())
}

#[test]
fn try_build_message_bytes_returns_message_byte_exact_when_data_holds_nul(
) -> Result<(), QuickFixError> {
    let dd = DataDictionary::try_from_path(FIX44)?;
    let wire = nul_raw_data();
    assert_round_trip(&dd.try_build_message_bytes(&wire)?, &wire, 96, b"a\0b");
    Ok(())
}

#[test]
fn try_from_bytes_returns_message_when_input_is_empty() -> Result<(), QuickFixError> {
    // Same as `try_from_text("")`: `QuickFIX` accepts an empty string as an empty message.
    Message::try_from_bytes(b"")?;
    Ok(())
}

#[test]
fn try_from_bytes_returns_error_when_frame_is_invalid() {
    for invalid in [&b"\x0050=18"[..], b"not fix", b"8=FIX.4.4\x019=5\x01\xFF"] {
        assert!(Message::try_from_bytes(invalid).is_err(), "{invalid:?}");
    }
}

#[test]
fn try_build_message_bytes_returns_error_when_frame_is_invalid() -> Result<(), QuickFixError> {
    let dd = DataDictionary::try_from_path(FIX44)?;
    let mut bad_checksum = latin1_text();
    let len = bad_checksum.len();
    bad_checksum[len - 2] ^= 1;
    for invalid in [&b""[..], b"\x0050=18", b"not fix", &bad_checksum] {
        assert!(dd.try_build_message_bytes(invalid).is_err(), "{invalid:?}");
    }
    Ok(())
}
