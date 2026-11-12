//! Creating, stopping and dropping acceptors and initiators (go-live phase 6).
//!
//! - R-P0-8: `MultiThreaded` is the default; `SingleThreaded` builds QuickFIX's plain socket
//!   classes, whose `stop()` crashes at random.
//! - R-P0-6: dropping an initiator or acceptor while another thread sends used to free the C++
//!   sessions under that send. Run under ASan to see the use-after-free.

use std::{
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, MutexGuard, PoisonError,
    },
    thread,
    time::{Duration, Instant},
};

use quickfix::*;
use utils::*;

mod utils;

const CYCLES: usize = 10;

fn serial() -> MutexGuard<'static, u8> {
    checker::GLOBAL_LOCK
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

#[test]
fn fix_socket_server_kind_default_is_multi_threaded() {
    assert_eq!(
        FixSocketServerKind::default(),
        FixSocketServerKind::MultiThreaded
    );
}

/// One pair's lifetime: start, log on, keep it up for a moment, then drop both sides.
fn run_pair_and_drop(port: u16) -> Result<bool, QuickFixError> {
    let settings_sender = build_settings(ServerType::Sender, port)?;
    let settings_receiver = build_settings(ServerType::Receiver, port)?;
    let (sender, receiver) = (NullFixApplication, NullFixApplication);
    let log = LogFactory::try_new(&NullLogger)?;
    let app_sender = Application::try_new(&sender)?;
    let app_receiver = Application::try_new(&receiver)?;
    let store = MemoryMessageStoreFactory::new();

    let kind = FixSocketServerKind::MultiThreaded;
    let mut acceptor = Acceptor::try_new(&settings_receiver, &app_receiver, &store, &log, kind)?;
    let mut initiator = Initiator::try_new(&settings_sender, &app_sender, &store, &log, kind)?;
    acceptor.start()?;
    initiator.start()?;

    let start = Instant::now();
    let mut logged_on = false;
    while !logged_on && start.elapsed() < Duration::from_secs(5) {
        logged_on = initiator.is_logged_on()? && acceptor.is_logged_on()?;
        thread::sleep(Duration::from_millis(10));
    }
    thread::sleep(Duration::from_millis(50));
    drop(initiator);
    drop(acceptor);
    Ok(logged_on)
}

#[test]
fn dropping_initiator_and_acceptor_while_another_thread_sends_is_safe() {
    let _lock = serial();
    let stop = Arc::new(AtomicBool::new(false));
    let sends = Arc::new(AtomicUsize::new(0));

    let sender = {
        let (stop, sends) = (Arc::clone(&stop), Arc::clone(&sends));
        thread::spawn(move || {
            let session = ServerType::Sender.session_id();
            while !stop.load(Ordering::Relaxed) {
                let news = build_news("while dropping", &[]).expect("news builds");
                // Errors are expected (no session between cycles); crashes are not.
                if send_to_target(news, &session).is_ok() {
                    sends.fetch_add(1, Ordering::Relaxed);
                }
            }
        })
    };

    let logged_on = (0..CYCLES)
        .filter(|_| run_pair_and_drop(find_available_port()).expect("pair runs"))
        .count();
    stop.store(true, Ordering::Relaxed);
    sender.join().expect("sender thread");

    assert_eq!(logged_on, CYCLES, "every cycle logged on");
    assert!(
        sends.load(Ordering::Relaxed) > 0,
        "some sends reached a live session"
    );
}
