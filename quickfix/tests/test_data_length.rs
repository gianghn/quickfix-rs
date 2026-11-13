//! R-P0-13: `QuickFIX` trusts a data field's length tag (upstream quickfix #779). A message with
//! `95=<n>` and a one-byte `96` makes the parser read `n` bytes from where `96` starts, so one
//! raw Logon kills the process.
//!
//! Fixed by the C++ fork's P0-1 (#779, pinned in phase 16): an oversized or negative length, in
//! any data field (95/96, 90/91 SecureData), is refused; the session never logs on and the
//! process lives.

use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{MutexGuard, PoisonError},
    thread,
    time::Duration,
};

use quickfix::{dictionary_item::CheckLatency, *};
use utils::{
    raw_peer::{contains, header, wire},
    *,
};

mod utils;

const FIX44: &str = "../quickfix-ffi/libquickfix/spec/FIX44.xml";

fn serial() -> MutexGuard<'static, u8> {
    checker::GLOBAL_LOCK
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

/// A Logon whose raw data (96) is one byte but whose raw data length (95) says `len`.
fn logon_with_data_length(mut fields: Vec<(i32, &'static [u8])>, len: &'static [u8]) -> Vec<u8> {
    fields.extend([(98, &b"0"[..]), (108, b"30"), (95, len), (96, b"x")]);
    wire(&fields)
}

/// Reads what the peer sends within a second; empty on a timeout or a closed socket.
fn read_some(peer: &mut impl Read) -> Vec<u8> {
    let mut buf = [0u8; 1024];
    let n = peer.read(&mut buf).unwrap_or(0);
    buf[..n].to_vec()
}

fn acceptor_refuses_logon(len: &'static [u8]) -> Result<(), QuickFixError> {
    let _lock = serial();
    let port = find_available_port();
    let mut settings = build_settings(ServerType::Receiver, port)?;
    settings.set(None, Dictionary::try_from_items(&[&CheckLatency(false)])?)?;
    let log = LogFactory::try_new(&NullLogger)?;
    let app_callbacks = NullFixApplication;
    let app = Application::try_new(&app_callbacks)?;
    let store = MemoryMessageStoreFactory::new();
    let mut acceptor = Acceptor::try_new(
        &settings,
        &app,
        &store,
        &log,
        FixSocketServerKind::MultiThreaded,
    )?;
    acceptor.start()?;
    thread::sleep(Duration::from_millis(100));

    let mut peer = TcpStream::connect(("127.0.0.1", port)).expect("acceptor listens");
    peer.set_read_timeout(Some(Duration::from_secs(1)))
        .expect("read timeout");
    peer.write_all(&logon_with_data_length(header(b"A", b"1"), len))
        .expect("logon sent");
    let reply = read_some(&mut peer);
    assert!(!contains(&reply, b"35=A\x01"), "logon accepted");
    assert!(!acceptor.is_logged_on()?);
    drop(peer);

    // Still serving: a well-formed Logon on a new connection is answered.
    let _peer = raw_peer::logon(port);
    Ok(())
}

fn initiator_refuses_logon_reply(len: &'static [u8]) -> Result<(), QuickFixError> {
    let _lock = serial();
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("listener binds");
    let port = listener.local_addr().expect("listener address").port();
    let mut settings = build_settings(ServerType::Sender, port)?;
    settings.set(None, Dictionary::try_from_items(&[&CheckLatency(false)])?)?;
    let log = LogFactory::try_new(&NullLogger)?;
    let app_callbacks = NullFixApplication;
    let app = Application::try_new(&app_callbacks)?;
    let store = MemoryMessageStoreFactory::new();
    let mut initiator = Initiator::try_new(
        &settings,
        &app,
        &store,
        &log,
        FixSocketServerKind::MultiThreaded,
    )?;
    initiator.start()?;

    let (mut peer, _) = listener.accept().expect("initiator connects");
    peer.set_read_timeout(Some(Duration::from_secs(5)))
        .expect("read timeout");
    assert!(contains(&read_some(&mut peer), b"35=A\x01"), "no logon");
    let reply = vec![
        (35, &b"A"[..]),
        (49, b"RECEIVER"),
        (56, b"SENDER"),
        (34, b"1"),
        (52, b"20261111-02:15:00.000"),
    ];
    peer.write_all(&logon_with_data_length(reply, len))
        .expect("logon reply sent");
    thread::sleep(Duration::from_secs(1));
    assert!(!initiator.is_logged_on()?);
    Ok(())
}

fn data_dictionary_refuses_message(len: &'static [u8]) -> Result<(), QuickFixError> {
    let dd = DataDictionary::try_from_path(FIX44)?;
    let text =
        String::from_utf8(logon_with_data_length(header(b"A", b"1"), len)).expect("ASCII message");
    assert!(dd.try_build_message(&text).is_err());
    Ok(())
}

fn data_dictionary_refuses_bytes(len: &'static [u8]) -> Result<(), QuickFixError> {
    let dd = DataDictionary::try_from_path(FIX44)?;
    let wire = logon_with_data_length(header(b"A", b"1"), len);
    assert!(dd.try_build_message_bytes(&wire).is_err());
    Ok(())
}

#[test]
fn acceptor_refuses_logon_when_95_is_1e5() -> Result<(), QuickFixError> {
    acceptor_refuses_logon(b"100000")
}

#[test]
fn acceptor_refuses_logon_when_95_is_1e8() -> Result<(), QuickFixError> {
    acceptor_refuses_logon(b"100000000")
}

#[test]
fn acceptor_refuses_logon_when_95_is_2e9() -> Result<(), QuickFixError> {
    acceptor_refuses_logon(b"2000000000")
}

#[test]
fn acceptor_refuses_logon_when_95_is_negative() -> Result<(), QuickFixError> {
    acceptor_refuses_logon(b"-50")
}

#[test]
fn initiator_refuses_logon_reply_when_95_is_1e5() -> Result<(), QuickFixError> {
    initiator_refuses_logon_reply(b"100000")
}

#[test]
fn initiator_refuses_logon_reply_when_95_is_1e8() -> Result<(), QuickFixError> {
    initiator_refuses_logon_reply(b"100000000")
}

#[test]
fn initiator_refuses_logon_reply_when_95_is_2e9() -> Result<(), QuickFixError> {
    initiator_refuses_logon_reply(b"2000000000")
}

#[test]
fn initiator_refuses_logon_reply_when_95_is_negative() -> Result<(), QuickFixError> {
    initiator_refuses_logon_reply(b"-50")
}

#[test]
fn try_build_message_returns_error_when_95_is_1e5() -> Result<(), QuickFixError> {
    data_dictionary_refuses_message(b"100000")
}

#[test]
fn try_build_message_returns_error_when_95_is_1e8() -> Result<(), QuickFixError> {
    data_dictionary_refuses_message(b"100000000")
}

#[test]
fn try_build_message_returns_error_when_95_is_2e9() -> Result<(), QuickFixError> {
    data_dictionary_refuses_message(b"2000000000")
}

#[test]
fn try_build_message_returns_error_when_95_is_negative() -> Result<(), QuickFixError> {
    data_dictionary_refuses_message(b"-50")
}

#[test]
fn try_build_message_bytes_returns_error_when_95_is_1e5() -> Result<(), QuickFixError> {
    data_dictionary_refuses_bytes(b"100000")
}

#[test]
fn try_build_message_bytes_returns_error_when_95_is_1e8() -> Result<(), QuickFixError> {
    data_dictionary_refuses_bytes(b"100000000")
}

#[test]
fn try_build_message_bytes_returns_error_when_95_is_2e9() -> Result<(), QuickFixError> {
    data_dictionary_refuses_bytes(b"2000000000")
}

#[test]
fn try_build_message_bytes_returns_error_when_95_is_negative() -> Result<(), QuickFixError> {
    data_dictionary_refuses_bytes(b"-50")
}

/// A Logon whose SecureData (91) is one byte but whose SecureDataLen (90) says `len`.
fn logon_with_secure_data_length(len: &'static [u8]) -> Vec<u8> {
    let mut fields = header(b"A", b"1");
    fields.extend([(90, len), (91, &b"x"[..]), (98, b"0"), (108, b"30")]);
    wire(&fields)
}

#[test]
fn try_build_message_bytes_returns_error_when_90_is_1e5() -> Result<(), QuickFixError> {
    let dd = DataDictionary::try_from_path(FIX44)?;
    assert!(dd
        .try_build_message_bytes(&logon_with_secure_data_length(b"100000"))
        .is_err());
    Ok(())
}

// Without a dictionary `QuickFIX` knows no data fields: 91 and 96 end at the next SOH, so the
// length is never trusted.
#[test]
fn try_from_bytes_ends_91_at_soh_when_90_is_1e5() -> Result<(), QuickFixError> {
    let msg = Message::try_from_bytes(&logon_with_secure_data_length(b"100000"))?;
    assert_eq!(msg.get_field_bytes(91), Some(b"x".to_vec()));
    Ok(())
}

#[test]
fn try_from_bytes_ends_96_at_soh_when_95_is_1e5() -> Result<(), QuickFixError> {
    let msg = Message::try_from_bytes(&logon_with_data_length(header(b"A", b"1"), b"100000"))?;
    assert_eq!(msg.get_field_bytes(96), Some(b"x".to_vec()));
    Ok(())
}

#[test]
fn try_build_message_bytes_parses_a_zero_length_data_field() -> Result<(), QuickFixError> {
    let dd = DataDictionary::try_from_path(FIX44)?;
    let mut fields = header(b"A", b"1");
    fields.extend([(98, &b"0"[..]), (108, b"30"), (95, b"0"), (96, b"")]);
    let msg = dd.try_build_message_bytes(&wire(&fields))?;
    assert_eq!(msg.get_field_bytes(96), Some(Vec::new()));
    Ok(())
}
