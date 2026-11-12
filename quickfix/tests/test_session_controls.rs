//! Go-live phase 10b, R-P2-2: small fixes with one test each.
//!
//! - `Dictionary::get` of an int or double returned `Ok(0)` when the key was missing or bad.
//! - Socket buffer sizes were `u16`, so no buffer above 64 KiB could be configured.
//! - `TimestampPrecision` had no typed item.
//! - Dropping an acceptor waited for the peer to answer the Logout (3 s measured, up to 10 s).
//! - `Session` had no `is_enabled`, `refresh` or on-demand TestRequest.

use std::{
    sync::{Mutex, MutexGuard, PoisonError},
    thread,
    time::{Duration, Instant},
};

use quickfix::{dictionary_item::*, *};
use utils::*;

mod utils;

fn serial() -> MutexGuard<'static, u8> {
    checker::GLOBAL_LOCK
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

#[test]
fn dictionary_get_number_of_missing_or_bad_key_is_an_error() -> Result<(), QuickFixError> {
    let mut dict = Dictionary::new();
    dict.set("text", "not a number")?;
    assert!(dict.get::<i32>("missing").is_err());
    assert!(dict.get::<f64>("missing").is_err());
    assert!(dict.get::<i32>("text").is_err());
    assert!(dict.get::<f64>("text").is_err());

    dict.set("int", 42)?;
    assert_eq!(dict.get::<i32>("int")?, 42);
    dict.set("zero", 0)?;
    assert_eq!(dict.get::<i32>("zero")?, 0);
    Ok(())
}

#[test]
fn socket_buffer_sizes_above_64_kib() -> Result<(), QuickFixError> {
    let dict = Dictionary::try_from_items(&[
        &SocketSendBufferSize(1_048_576),
        &SocketReceiveBufferSize(4_194_304),
    ])?;
    assert_eq!(dict.get::<i32>("SocketSendBufferSize")?, 1_048_576);
    assert_eq!(dict.get::<i32>("SocketReceiveBufferSize")?, 4_194_304);

    // SO_SNDBUF is a C int: a size it cannot hold is refused, not wrapped.
    assert!(Dictionary::try_from_items(&[&SocketSendBufferSize(u32::MAX)]).is_err());
    Ok(())
}

#[test]
fn timestamp_precision_is_typed() -> Result<(), QuickFixError> {
    for (item, digits) in [
        (TimestampPrecision::Seconds, 0),
        (TimestampPrecision::Milliseconds, 3),
        (TimestampPrecision::Microseconds, 6),
        (TimestampPrecision::Nanoseconds, 9),
    ] {
        let dict = Dictionary::try_from_items(&[&item])?;
        assert_eq!(dict.get::<i32>("TimestampPrecision")?, digits);
    }
    Ok(())
}

#[test]
fn drop_does_not_wait_for_an_unanswered_logout() -> Result<(), QuickFixError> {
    let _lock = serial();
    let port = find_available_port();
    let mut settings = build_settings(ServerType::Receiver, port)?;
    // The raw peer's SendingTime is fixed. A long LogoutTimeout keeps the unanswered session
    // logged on for the whole stop, like a counterparty that has gone silent.
    settings.set(
        None,
        Dictionary::try_from_items(&[&CheckLatency(false), &LogoutTimeout(30)])?,
    )?;
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

    // Logged on, and never answers the Logout the acceptor sends when it stops.
    let _peer = raw_peer::logon(port);
    assert!(checker::wait_until(Duration::from_secs(5), || acceptor
        .is_logged_on()
        .unwrap_or(false)));

    let started = Instant::now();
    drop(acceptor);
    let took = started.elapsed();
    // A graceful stop waits 10 s for the Logout answers, then QuickFIX's threaded acceptor waits
    // up to 5 s more in onStop whatever `force` says (a C++ fork item). Drop skips the 10 s.
    assert!(took < Duration::from_secs(7), "drop took {took:?}");
    Ok(())
}

/// Admin messages the initiator side received: MsgType (35) and TestReqID (112).
#[derive(Debug, Default)]
struct AdminInbox(Mutex<Vec<(String, Option<String>)>>);

impl AdminInbox {
    fn heartbeat_for(&self, test_req_id: &str) -> bool {
        let all = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        all.iter()
            .any(|(msg_type, id)| msg_type == "0" && id.as_deref() == Some(test_req_id))
    }
}

impl ApplicationCallback for AdminInbox {
    fn on_msg_from_admin(&self, msg: &Message, _: &SessionId) -> Result<(), MsgFromAdminError> {
        let msg_type = msg.with_header(|h| h.get_field(35)).unwrap_or_default();
        let mut all = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        all.push((msg_type, msg.get_field(112)));
        Ok(())
    }
}

#[test]
fn session_is_enabled_refresh_and_test_request() -> Result<(), QuickFixError> {
    let _lock = serial();
    let port = find_available_port();
    let settings_sender = build_settings(ServerType::Sender, port)?;
    let settings_receiver = build_settings(ServerType::Receiver, port)?;
    let (sender, receiver) = (AdminInbox::default(), NullFixApplication);
    let log = LogFactory::try_new(&NullLogger)?;
    let app_sender = Application::try_new(&sender)?;
    let app_receiver = Application::try_new(&receiver)?;
    let (store_sender, store_receiver) = (
        MemoryMessageStoreFactory::new(),
        MemoryMessageStoreFactory::new(),
    );
    let kind = FixSocketServerKind::MultiThreaded;
    let mut acceptor = Acceptor::try_new(
        &settings_receiver,
        &app_receiver,
        &store_receiver,
        &log,
        kind,
    )?;
    let mut initiator =
        Initiator::try_new(&settings_sender, &app_sender, &store_sender, &log, kind)?;
    acceptor.start()?;
    initiator.start()?;
    assert!(checker::wait_until(Duration::from_secs(10), || {
        initiator.is_logged_on().unwrap_or(false) && acceptor.is_logged_on().unwrap_or(false)
    }));

    {
        let mut session = initiator.session(ServerType::Sender.session_id())?;
        assert!(session.is_enabled()?);
        session.refresh()?;

        // The counterparty answers a TestRequest with a Heartbeat carrying the same TestReqID.
        assert!(session.send_test_request("PING-1")?);
        assert!(checker::wait_until(Duration::from_secs(5), || sender.heartbeat_for("PING-1")));

        session.logout()?;
        assert!(!session.is_enabled()?);
    }

    initiator.stop()?;
    acceptor.stop()?;
    Ok(())
}
