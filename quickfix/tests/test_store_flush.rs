//! Phase 21a: the C++ fork's background flush (`FileStoreFlushInterval`, production setting with
//! `FileStoreSync=N`) works through the binding with no API change: a session on a file store
//! with a 5 ms flush sends, stores and stops cleanly (the flusher thread is joined on drop).

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

#[derive(Default)]
struct LogonFlag(AtomicBool);

impl ApplicationCallback for LogonFlag {
    fn on_logon(&self, _session: &SessionId) {
        self.0.store(true, Ordering::SeqCst);
    }
}

fn receives(peer: &mut TcpStream, needle: &[u8]) -> bool {
    let mut seen = Vec::new();
    let mut buf = [0u8; 4096];
    while let Ok(n @ 1..) = peer.read(&mut buf) {
        seen.extend_from_slice(&buf[..n]);
        if contains(&seen, needle) {
            return true;
        }
    }
    false
}

#[test]
fn a_file_store_with_background_flush_sends_stores_and_stops_cleanly() -> Result<(), QuickFixError>
{
    let _lock = checker::GLOBAL_LOCK
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    let port = find_available_port();
    let dir = env::temp_dir().join(format!("qfrs-flush-{}", std::process::id()));
    fs::remove_dir_all(&dir).ok();
    let path = dir.to_str().expect("UTF-8 temp dir");

    let mut settings = build_settings(ServerType::Receiver, port)?;
    // The raw peer's Logon carries a fixed SendingTime.
    let mut defaults = Dictionary::try_from_items(&[&CheckLatency(false), &FileStorePath(path)])?;
    defaults.set("FileStoreSync", false)?;
    defaults.set("FileStoreFlushInterval", 5)?;
    settings.set(None, defaults)?;

    let flag = LogonFlag::default();
    let log = LogFactory::try_new(&NullLogger)?;
    let app = Application::try_new(&flag)?;
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

    let mut peer = raw_peer::logon(port);
    assert!(wait_until(Duration::from_secs(5), || flag
        .0
        .load(Ordering::SeqCst)));
    let id = ServerType::Receiver.session_id();
    for i in 0..20 {
        send_to_target(build_news(&format!("flushed{i}"), &[])?, &id)?;
    }
    assert!(receives(&mut peer, b"148=flushed19\x01"));
    // Several flush intervals, so the flusher has run while the session is up.
    thread::sleep(Duration::from_millis(50));

    acceptor.stop()?;
    drop(acceptor);

    let body = fs::read_dir(&dir)
        .expect("store dir")
        .filter_map(Result::ok)
        .find(|e| e.path().extension().is_some_and(|x| x == "body"))
        .expect("a .body file");
    let stored = fs::read(body.path()).expect("body readable");
    assert!(contains(&stored, b"148=flushed19\x01"));
    fs::remove_dir_all(&dir).ok();
    Ok(())
}
