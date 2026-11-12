//! Bytes that are not plain UTF-8 text survive the binding unchanged (go-live phase 9).
//!
//! - R-P1-3: log callbacks got `const char*`, so text stopped at the first NUL, and a message that
//!   was not UTF-8 was replaced by a placeholder: holes in the audit trail.
//! - R-P1-4: `to_fix_bytes` copied with `strncpy`, zero-filling everything after an embedded NUL.
//!
//! A raw TCP peer logs on to an acceptor and sends a News with `58=caf\xE9` and `96=a\0b`.

use std::{
    io::Write,
    sync::{Mutex, MutexGuard, PoisonError},
    thread,
    time::Duration,
};

use quickfix::{dictionary_item::CheckLatency, *};
use utils::{
    raw_peer::{contains, header, wire},
    *,
};

mod utils;

fn serial() -> MutexGuard<'static, u8> {
    checker::GLOBAL_LOCK
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
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
    let mut peer = raw_peer::logon(port);

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
