//! Phase 20 (D19): `SessionId` is `Send + Sync`, so an app (itself `Send + Sync`, R-P0-4) can keep
//! the session it logged on with and send through it from any thread.

use std::{
    io::Read,
    net::TcpStream,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    thread,
    time::Duration,
};

use quickfix::{dictionary_item::CheckLatency, *};
use utils::{checker::wait_until, raw_peer::contains, *};

mod utils;

fn assert_send_sync<T: Send + Sync>() {}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Keeps a copy of the session it last logged on with, as a gateway does.
#[derive(Default)]
struct Keeper {
    session: Mutex<Option<SessionId>>,
}

impl ApplicationCallback for Keeper {
    fn on_logon(&self, session: &SessionId) {
        *lock(&self.session) = Some(session.clone());
    }
}

/// Reads from the peer until `needle` arrives or the socket's read timeout passes.
fn receives(peer: &mut TcpStream, needle: &[u8]) -> bool {
    let mut seen = Vec::new();
    let mut buf = [0u8; 1024];
    while let Ok(n @ 1..) = peer.read(&mut buf) {
        seen.extend_from_slice(&buf[..n]);
        if contains(&seen, needle) {
            return true;
        }
    }
    false
}

#[test]
fn session_id_is_send_and_sync() {
    assert_send_sync::<SessionId>();
}

#[test]
fn session_id_kept_in_the_app_sends_from_another_thread() -> Result<(), QuickFixError> {
    let _lock = lock(&checker::GLOBAL_LOCK);
    let port = find_available_port();
    let mut settings = build_settings(ServerType::Receiver, port)?;
    // The raw peer's Logon carries a fixed SendingTime.
    settings.set(None, Dictionary::try_from_items(&[&CheckLatency(false)])?)?;
    let log = LogFactory::try_new(&NullLogger)?;
    let keeper = Keeper::default();
    let app = Application::try_new(&keeper)?;
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

    let mut peer = raw_peer::logon(port);
    assert!(wait_until(Duration::from_secs(5), || lock(&keeper.session).is_some()));
    let session = lock(&keeper.session).clone().expect("kept on logon");

    let sender = thread::spawn(move || send_to_target(build_news("from-thread", &[])?, &session));
    sender.join().expect("sender thread")?;

    assert!(receives(&mut peer, b"148=from-thread\x01"));
    Ok(())
}

#[test]
fn session_id_shared_by_threads_reads_the_same() -> Result<(), QuickFixError> {
    let session = Arc::new(SessionId::try_new("FIX.4.4", "SENDER", "RECEIVER", "KRX")?);
    let expected = session.to_repr();

    let readers: Vec<_> = (0..8)
        .map(|_| {
            let session = Arc::clone(&session);
            thread::spawn(move || {
                (0..500).all(|_| {
                    let copy = session.clone();
                    session.get_begin_string().as_deref() == Some("FIX.4.4")
                        && session.get_sender_comp_id().as_deref() == Some("SENDER")
                        && session.get_target_comp_id().as_deref() == Some("RECEIVER")
                        && session.get_session_qualifier().as_deref() == Some("KRX")
                        && !session.is_fixt()
                        && copy.to_repr() == session.to_repr()
                })
            })
        })
        .collect();

    for reader in readers {
        assert!(reader.join().expect("reader thread"));
    }
    assert_eq!(session.to_repr(), expected);
    Ok(())
}

#[test]
fn session_id_drops_on_another_thread() -> Result<(), QuickFixError> {
    let session = SessionId::try_new("FIX.4.4", "SENDER", "RECEIVER", "")?;
    let repr = thread::spawn(move || session.to_repr())
        .join()
        .expect("dropping thread");
    assert_eq!(repr, "FIX.4.4:SENDER->RECEIVER");
    Ok(())
}
