//! Phase 21b (D23): `send_batch_to_target` over the C++ fork's batch send (15.7). It returns how
//! many messages went out, from the first; `messages[n..]` were not sent.

use std::{
    env, fs,
    io::Read,
    net::TcpStream,
    sync::{atomic::AtomicBool, atomic::Ordering, PoisonError},
    thread,
    time::Duration,
};

use quickfix::{
    dictionary_item::{CheckLatency, FileStorePath},
    *,
};
use utils::{checker::wait_until, raw_peer::contains, *};

mod utils;

/// Records the logon; refuses (`DoNotSend`) the message whose headline is `refuse`.
#[derive(Default)]
struct Gate {
    logged_on: AtomicBool,
}

impl ApplicationCallback for Gate {
    fn on_logon(&self, _session: &SessionId) {
        self.logged_on.store(true, Ordering::SeqCst);
    }

    fn on_msg_to_app(
        &self,
        msg: &mut MessageMut<'_>,
        _session: &SessionId,
    ) -> Result<(), MsgToAppError> {
        if msg.get_field(148).as_deref() == Some("refuse") {
            return Err(MsgToAppError::DoNotSend);
        }
        Ok(())
    }
}

/// Reads from the peer until `needle` arrives or the read times out; returns all it read.
fn read_until(peer: &mut TcpStream, needle: &[u8]) -> Vec<u8> {
    let mut seen = Vec::new();
    let mut buf = [0u8; 4096];
    while let Ok(n @ 1..) = peer.read(&mut buf) {
        seen.extend_from_slice(&buf[..n]);
        if contains(&seen, needle) {
            break;
        }
    }
    seen
}

fn headline(text: &str) -> Vec<u8> {
    format!("148={text}\x01").into_bytes()
}

fn news(texts: &[&str]) -> Result<Vec<Message>, QuickFixError> {
    texts.iter().map(|t| build_news(t, &[])).collect()
}

/// A fresh store directory for one test.
fn store_dir(test: &str) -> String {
    let dir = env::temp_dir().join(format!("qfrs-batch-{}-{test}", std::process::id()));
    fs::remove_dir_all(&dir).ok();
    dir.to_str().expect("UTF-8 temp dir").to_owned()
}

/// Runs `body` with a started acceptor for `gate` on a file store in `dir` (`FileStoreSync` =
/// `sync`).
fn with_acceptor(
    gate: &Gate,
    dir: &str,
    sync: bool,
    body: impl FnOnce(u16) -> Result<(), QuickFixError>,
) -> Result<(), QuickFixError> {
    let _lock = checker::GLOBAL_LOCK
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    let port = find_available_port();
    let mut settings = build_settings(ServerType::Receiver, port)?;
    // The raw peer's Logon carries a fixed SendingTime.
    let mut defaults = Dictionary::try_from_items(&[&CheckLatency(false), &FileStorePath(dir)])?;
    defaults.set("FileStoreSync", sync)?;
    settings.set(None, defaults)?;
    let log = LogFactory::try_new(&NullLogger)?;
    let app = Application::try_new(gate)?;
    let store = FileMessageStoreFactory::try_new(&settings)?;
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

fn logged_on(gate: &Gate, port: u16) -> TcpStream {
    let peer = raw_peer::logon(port);
    assert!(wait_until(Duration::from_secs(5), || gate
        .logged_on
        .load(Ordering::SeqCst)));
    peer
}

#[test]
fn batch_on_a_logged_on_session_sends_all_in_order() -> Result<(), QuickFixError> {
    let gate = Gate::default();
    with_acceptor(&gate, &store_dir("order"), false, |port| {
        let mut peer = logged_on(&gate, port);
        let batch = news(&["b0", "b1", "b2", "b3", "b4"])?;
        let sent = send_batch_to_target(&batch, &ServerType::Receiver.session_id())?;
        assert_eq!(sent, 5);
        let wire = read_until(&mut peer, &headline("b4"));
        let at = |t: &str| {
            wire.windows(headline(t).len())
                .position(|w| w == headline(t).as_slice())
        };
        let positions: Vec<_> = ["b0", "b1", "b2", "b3", "b4"].map(at).into();
        assert!(positions.iter().all(Option::is_some), "got {positions:?}");
        assert!(positions.is_sorted(), "out of order: {positions:?}");
        Ok(())
    })
}

#[test]
fn batch_while_logged_off_sends_none() -> Result<(), QuickFixError> {
    let gate = Gate::default();
    with_acceptor(&gate, &store_dir("logged-off"), false, |_port| {
        let batch = news(&["x0", "x1"])?;
        let sent = send_batch_to_target(&batch, &ServerType::Receiver.session_id())?;
        assert_eq!(sent, 0);
        Ok(())
    })
}

#[test]
fn batch_stops_at_the_first_refused_message() -> Result<(), QuickFixError> {
    let gate = Gate::default();
    with_acceptor(&gate, &store_dir("refused"), false, |port| {
        let mut peer = logged_on(&gate, port);
        let batch = news(&["r0", "r1", "refuse", "r3"])?;
        let sent = send_batch_to_target(&batch, &ServerType::Receiver.session_id())?;
        assert_eq!(sent, 2);
        // A marker sent after the batch: everything before it has arrived when it does.
        send_to_target(
            build_news("marker", &[])?,
            &ServerType::Receiver.session_id(),
        )?;
        let wire = read_until(&mut peer, &headline("marker"));
        assert!(contains(&wire, &headline("r0")) && contains(&wire, &headline("r1")));
        assert!(!contains(&wire, &headline("refuse")) && !contains(&wire, &headline("r3")));
        Ok(())
    })
}

#[test]
fn batch_to_an_unknown_session_is_session_not_found() -> Result<(), QuickFixError> {
    let unknown = SessionId::try_new("FIX.4.4", "NOBODY", "NOWHERE", "")?;
    let sent = send_batch_to_target(&news(&["lost"])?, &unknown);
    assert!(
        matches!(sent, Err(QuickFixError::SessionNotFound(_))),
        "got {sent:?}"
    );
    Ok(())
}

#[test]
fn empty_batch_sends_none() -> Result<(), QuickFixError> {
    let unknown = SessionId::try_new("FIX.4.4", "NOBODY", "NOWHERE", "")?;
    assert_eq!(send_batch_to_target(&[], &unknown)?, 0);
    Ok(())
}

#[test]
fn batch_with_sync_store_stores_and_sends_all() -> Result<(), QuickFixError> {
    let dir = store_dir("sync");
    let gate = Gate::default();
    with_acceptor(&gate, &dir, true, |port| {
        let mut peer = logged_on(&gate, port);
        let texts: Vec<String> = (0..64).map(|i| format!("s{i}")).collect();
        let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
        let sent = send_batch_to_target(&news(&refs)?, &ServerType::Receiver.session_id())?;
        assert_eq!(sent, 64);
        assert!(contains(
            &read_until(&mut peer, &headline("s63")),
            &headline("s63")
        ));
        Ok(())
    })?;
    let body = fs::read_dir(&dir)
        .expect("store dir")
        .filter_map(Result::ok)
        .find(|e| e.path().extension().is_some_and(|x| x == "body"))
        .expect("a .body file");
    let stored = fs::read(body.path()).expect("body readable");
    assert!(contains(&stored, &headline("s0")) && contains(&stored, &headline("s63")));
    fs::remove_dir_all(&dir).ok();
    Ok(())
}
