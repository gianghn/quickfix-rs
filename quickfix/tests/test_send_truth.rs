//! Phase 17 (R-P0-12): `send_to_target` returns `Ok` only when `QuickFIX` wrote the message now.
//! A message it stored but did not write (session down, write failed) or that `on_msg_to_app`
//! refused is `QuickFixError::NotSent`; a stored one may still go out later as a PossDup resend.

use std::{
    io::Read,
    net::TcpStream,
    sync::{atomic::AtomicBool, atomic::Ordering, Mutex, MutexGuard, PoisonError},
    thread,
    time::Duration,
};

use quickfix::{dictionary_item::CheckLatency, *};
use utils::{checker::wait_until, raw_peer::contains, *};

mod utils;

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Records the logon; refuses every application message when `refuse` is set.
#[derive(Default)]
struct Gate {
    logged_on: AtomicBool,
    refuse: bool,
}

impl ApplicationCallback for Gate {
    fn on_logon(&self, _session: &SessionId) {
        self.logged_on.store(true, Ordering::SeqCst);
    }

    fn on_msg_to_app(
        &self,
        _msg: &mut MessageMut<'_>,
        _session: &SessionId,
    ) -> Result<(), MsgToAppError> {
        if self.refuse {
            return Err(MsgToAppError::DoNotSend);
        }
        Ok(())
    }
}

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

/// Runs `body` with an acceptor for `gate` started on a free port.
fn with_acceptor(
    gate: &Gate,
    body: impl FnOnce(u16) -> Result<(), QuickFixError>,
) -> Result<(), QuickFixError> {
    let _lock = lock(&checker::GLOBAL_LOCK);
    let port = find_available_port();
    let mut settings = build_settings(ServerType::Receiver, port)?;
    // The raw peer's Logon carries a fixed SendingTime.
    settings.set(None, Dictionary::try_from_items(&[&CheckLatency(false)])?)?;
    let log = LogFactory::try_new(&NullLogger)?;
    let app = Application::try_new(gate)?;
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
    body(port)
}

#[test]
fn send_to_target_is_not_sent_while_the_session_is_logged_off() -> Result<(), QuickFixError> {
    let gate = Gate::default();
    with_acceptor(&gate, |_port| {
        let sent = send_to_target(
            build_news("nobody there", &[])?,
            &ServerType::Receiver.session_id(),
        );
        assert!(
            matches!(sent, Err(QuickFixError::NotSent(_))),
            "got {sent:?}"
        );
        Ok(())
    })
}

#[test]
fn send_to_target_is_not_sent_when_on_msg_to_app_says_do_not_send() -> Result<(), QuickFixError> {
    let gate = Gate {
        refuse: true,
        ..Gate::default()
    };
    with_acceptor(&gate, |port| {
        let mut peer = raw_peer::logon(port);
        assert!(wait_until(Duration::from_secs(5), || gate
            .logged_on
            .load(Ordering::SeqCst)));
        let sent = send_to_target(
            build_news("refused", &[])?,
            &ServerType::Receiver.session_id(),
        );
        assert!(
            matches!(sent, Err(QuickFixError::NotSent(_))),
            "got {sent:?}"
        );
        assert!(!receives(&mut peer, b"148=refused\x01"));
        Ok(())
    })
}

#[test]
fn send_to_target_is_ok_when_the_message_is_written() -> Result<(), QuickFixError> {
    let gate = Gate::default();
    with_acceptor(&gate, |port| {
        let mut peer = raw_peer::logon(port);
        assert!(wait_until(Duration::from_secs(5), || gate
            .logged_on
            .load(Ordering::SeqCst)));
        send_to_target(
            build_news("written", &[])?,
            &ServerType::Receiver.session_id(),
        )?;
        assert!(receives(&mut peer, b"148=written\x01"));
        Ok(())
    })
}

#[test]
fn send_to_target_to_an_unknown_session_is_session_not_found() -> Result<(), QuickFixError> {
    let unknown = SessionId::try_new("FIX.4.4", "NOBODY", "NOWHERE", "")?;
    let sent = send_to_target(build_news("lost", &[])?, &unknown);
    assert!(
        matches!(sent, Err(QuickFixError::SessionNotFound(_))),
        "got {sent:?}"
    );
    Ok(())
}
