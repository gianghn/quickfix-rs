//! Bytes that are not plain UTF-8 text survive the binding unchanged (go-live phase 9).
//!
//! - R-P1-3: log callbacks got `const char*`, so text stopped at the first NUL, and a message that
//!   was not UTF-8 was replaced by a placeholder: holes in the audit trail.
//! - R-P1-4: `to_fix_bytes` copied with `strncpy`, zero-filling everything after an embedded NUL.
//!
//! A raw TCP peer logs on to an acceptor and sends a News with `58=caf\xE9` and `96=a\0b`.

use std::{
    io::{Read, Write},
    net::TcpStream,
    sync::{Mutex, MutexGuard, PoisonError},
    thread,
    time::Duration,
};

use quickfix::{dictionary_item::CheckLatency, *};
use utils::*;

mod utils;

const SOH: u8 = 1;

fn serial() -> MutexGuard<'static, u8> {
    checker::GLOBAL_LOCK
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

/// Frames `fields` as FIX 4.4 wire bytes with a correct BodyLength and CheckSum.
fn wire(fields: &[(i32, &[u8])]) -> Vec<u8> {
    let mut body = Vec::new();
    for (tag, value) in fields {
        body.extend_from_slice(format!("{tag}=").as_bytes());
        body.extend_from_slice(value);
        body.push(SOH);
    }
    let mut out = format!("8=FIX.4.4\x019={}\x01", body.len()).into_bytes();
    out.extend_from_slice(&body);
    let sum = out.iter().map(|b| u32::from(*b)).sum::<u32>() % 256;
    out.extend_from_slice(format!("10={sum:03}\x01").as_bytes());
    out
}

fn header(msg_type: &'static [u8], seq: &'static [u8]) -> Vec<(i32, &'static [u8])> {
    vec![
        (35, msg_type),
        (49, b"SENDER"),
        (56, b"RECEIVER"),
        (34, seq),
        (52, b"20261111-02:15:00.000"),
    ]
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

#[derive(Debug, Default)]
struct Received {
    log_incoming: Mutex<Vec<Vec<u8>>>,
    raw_data: Mutex<Option<Vec<u8>>>,
    message: Mutex<Option<Vec<u8>>>,
}

impl LogCallback for Received {
    fn on_incoming_bytes(&self, _: Option<&SessionId>, msg: &[u8]) {
        let mut all = self
            .log_incoming
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        all.push(msg.to_vec());
    }
}

impl ApplicationCallback for Received {
    fn on_msg_from_app(&self, msg: &Message, _: &SessionId) -> Result<(), MsgFromAppError> {
        *self.raw_data.lock().unwrap_or_else(PoisonError::into_inner) = msg.get_field_bytes(96);
        *self.message.lock().unwrap_or_else(PoisonError::into_inner) = msg.to_fix_bytes().ok();
        Ok(())
    }
}

/// Logs on as SENDER, sends the News, and returns once the acceptor handled it (or gave up).
fn send_news(port: u16, received: &Received) {
    let mut peer = TcpStream::connect(("127.0.0.1", port)).expect("acceptor listens");
    peer.set_read_timeout(Some(Duration::from_secs(5))).unwrap();

    let mut logon = header(b"A", b"1");
    logon.extend([(98, &b"0"[..]), (108, b"30")]);
    peer.write_all(&wire(&logon)).unwrap();
    let mut reply = [0u8; 512];
    let n = peer.read(&mut reply).expect("logon answered");
    assert!(contains(&reply[..n], b"35=A\x01"), "no logon reply");

    let mut news = header(b"B", b"2");
    news.extend([
        (148, &b"bytes"[..]),
        (33, b"1"),
        (58, b"caf\xE9"),
        (95, b"3"),
        (96, b"a\x00b"),
    ]);
    peer.write_all(&wire(&news)).unwrap();
    checker::wait_until(Duration::from_secs(5), || {
        received
            .message
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_some()
    });
}

#[test]
fn log_message_and_field_bytes_equal_the_wire() -> Result<(), QuickFixError> {
    let _lock = serial();
    let port = find_available_port();
    let mut settings = build_settings(ServerType::Receiver, port)?;
    settings.set(None, Dictionary::try_from_items(&[&CheckLatency(false)])?)?;
    let received = Received::default();
    let log = LogFactory::try_new(&received)?;
    let app = Application::try_new(&received)?;
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

    send_news(port, &received);
    acceptor.stop()?;

    let logged = received
        .log_incoming
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    let news = logged
        .iter()
        .find(|m| contains(m, b"35=B\x01"))
        .expect("News logged");
    assert!(
        contains(news, b"\x0158=caf\xE9\x01"),
        "log lost the non-UTF-8 text"
    );
    assert!(
        contains(news, b"\x0196=a\x00b\x01"),
        "log stopped at the NUL"
    );

    let raw = received
        .raw_data
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    assert_eq!(raw.as_deref(), Some(&b"a\x00b"[..]));
    let message = received
        .message
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    let message = message.as_deref().expect("News handled");
    assert!(
        contains(message, b"\x0196=a\x00b\x01"),
        "to_fix_bytes zero-filled after the NUL"
    );
    Ok(())
}
