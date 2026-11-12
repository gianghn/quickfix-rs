//! Sends from inside `on_msg_from_app` (go-live phase 11, ported from the research harness's
//! `reenter` and `cross` modes).
//!
//! The callback runs on the C++ connection thread while that session is in `next()`. A reply
//! on the same session re-enters it; a forward to another session takes that session's lock while
//! the other connection thread may be doing the same the other way round. Neither may deadlock or
//! lose a message while client threads keep sending.

use std::{
    io::{self, Write},
    process,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, MutexGuard, PoisonError,
    },
    thread,
    time::Duration,
};

use quickfix::*;
use utils::*;

mod utils;

const THREADS_PER_SESSION: usize = 2;
const SENDS_PER_THREAD: usize = 200;

fn serial() -> MutexGuard<'static, u8> {
    checker::GLOBAL_LOCK
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

/// Where the receiving side sends one message for each message it gets.
#[derive(Debug, Clone, Copy)]
enum Route {
    /// Nowhere: the client side only counts.
    Keep,
    /// Back on the session it came in on.
    Back,
    /// On the acceptor's other session (SENDER <-> SENDER2).
    Across,
}

#[derive(Debug)]
struct Router {
    route: Route,
    logons: AtomicUsize,
    received: AtomicUsize,
    failed_sends: AtomicUsize,
}

impl Router {
    fn new(route: Route) -> Self {
        Self {
            route,
            logons: AtomicUsize::new(0),
            received: AtomicUsize::new(0),
            failed_sends: AtomicUsize::new(0),
        }
    }
}

impl ApplicationCallback for Router {
    fn on_logon(&self, _: &SessionId) {
        self.logons.fetch_add(1, Ordering::SeqCst);
    }

    fn on_msg_from_app(&self, _: &Message, session: &SessionId) -> Result<(), MsgFromAppError> {
        self.received.fetch_add(1, Ordering::SeqCst);
        let target = match self.route {
            Route::Keep => return Ok(()),
            Route::Back => session.clone(),
            Route::Across => {
                let other = match session.get_target_comp_id().as_deref() {
                    Some("SENDER") => "SENDER2",
                    _ => "SENDER",
                };
                SessionId::try_new("FIX.4.4", "RECEIVER", other, "").expect("session id")
            }
        };
        let reply = build_news("reply", &[]).expect("news builds");
        if send_to_target(reply, &target).is_err() {
            self.failed_sends.fetch_add(1, Ordering::SeqCst);
        }
        Ok(())
    }
}

/// Sends `SENDS_PER_THREAD` News from `sender` to RECEIVER on a new thread, counting each `Ok`.
fn spawn_sender(sender: &'static str, sent: Arc<AtomicUsize>) {
    // A SessionId is not Send: each thread builds its own.
    thread::spawn(move || {
        let id = SessionId::try_new("FIX.4.4", sender, "RECEIVER", "").expect("session id");
        for _ in 0..SENDS_PER_THREAD {
            let news = build_news("request", &[]).expect("news builds");
            if send_to_target(news, &id).is_ok() {
                sent.fetch_add(1, Ordering::SeqCst);
            }
        }
    });
}

/// Client sessions SENDER (and SENDER2 when `sessions` is 2) send to RECEIVER, which routes a
/// reply for each message; every message and every reply must arrive.
fn run(route: Route, sessions: usize) -> Result<(), QuickFixError> {
    let _lock = serial();
    let port = find_available_port();
    let mut settings_client = build_settings(ServerType::Sender, port)?;
    let mut settings_server = build_settings(ServerType::Receiver, port)?;
    let senders = &["SENDER", "SENDER2"][..sessions];
    if sessions == 2 {
        let client = SessionId::try_new("FIX.4.4", "SENDER2", "RECEIVER", "")?;
        let server = SessionId::try_new("FIX.4.4", "RECEIVER", "SENDER2", "")?;
        add_session(&mut settings_client, &client, port)?;
        add_session(&mut settings_server, &server, port)?;
    }

    let (client, server) = (Router::new(Route::Keep), Router::new(route));
    let log = LogFactory::try_new(&NullLogger)?;
    let app_client = Application::try_new(&client)?;
    let app_server = Application::try_new(&server)?;
    let (store_client, store_server) = (
        MemoryMessageStoreFactory::new(),
        MemoryMessageStoreFactory::new(),
    );
    let kind = FixSocketServerKind::MultiThreaded;
    let mut acceptor = Acceptor::try_new(&settings_server, &app_server, &store_server, &log, kind)?;
    let mut initiator =
        Initiator::try_new(&settings_client, &app_client, &store_client, &log, kind)?;
    acceptor.start()?;
    initiator.start()?;
    assert!(
        checker::wait_until(Duration::from_secs(10), || {
            client.logons.load(Ordering::SeqCst) == sessions
                && server.logons.load(Ordering::SeqCst) == sessions
        }),
        "every session logged on"
    );

    // Detached senders: a deadlock must fail the test, not hang it in a join.
    let total = sessions * THREADS_PER_SESSION * SENDS_PER_THREAD;
    let sent = Arc::new(AtomicUsize::new(0));
    for &sender in senders {
        for _ in 0..THREADS_PER_SESSION {
            spawn_sender(sender, Arc::clone(&sent));
        }
    }
    let done = checker::wait_until(Duration::from_secs(30), || {
        server.received.load(Ordering::SeqCst) == total
            && client.received.load(Ordering::SeqCst) == total
    });
    if !done {
        // Unwinding would stop the handlers, which joins the stuck QuickFIX threads.
        writeln!(
            io::stderr(),
            "deadlock: sent {} of {total}, server got {}, client got {} replies",
            sent.load(Ordering::SeqCst),
            server.received.load(Ordering::SeqCst),
            client.received.load(Ordering::SeqCst),
        )
        .ok();
        process::abort();
    }
    assert_eq!(sent.load(Ordering::SeqCst), total);
    assert_eq!(server.failed_sends.load(Ordering::SeqCst), 0);
    initiator.stop()?;
    acceptor.stop()?;
    Ok(())
}

#[test]
fn send_from_from_app_on_the_same_session_does_not_deadlock() -> Result<(), QuickFixError> {
    run(Route::Back, 1)
}

#[test]
fn send_from_from_app_to_another_session_does_not_deadlock() -> Result<(), QuickFixError> {
    run(Route::Across, 2)
}
