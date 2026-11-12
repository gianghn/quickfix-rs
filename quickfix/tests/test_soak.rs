//! Opt-in soak runs, ported from the research harness's `stress` and `latency` modes (go-live
//! phase 11). Too slow for every CI run, so both are `#[ignore]`d; run them with
//!
//! ```text
//! QFRS_SOAK_SECS=300 cargo test -p quickfix --release --test test_soak -- --ignored --nocapture
//! ```
//!
//! `QFRS_SOAK_SECS` sets the chaos length (default 60), `QFRS_LATENCY_N` the round trips
//! (default 10000).

use std::{
    env, fs,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{sync_channel, SyncSender},
        Arc, Mutex, MutexGuard, PoisonError,
    },
    thread,
    time::{Duration, Instant},
};

use quickfix::{dictionary_item::FileStorePath, *};
use utils::*;

mod utils;

const SENDER_THREADS: usize = 2;

fn serial() -> MutexGuard<'static, u8> {
    checker::GLOBAL_LOCK
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

fn env_or(name: &str, default: u64) -> u64 {
    env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// Records, per sender thread, the numbers of the messages it received (Headline `t<i>.n<k>`).
#[derive(Debug, Default)]
struct Inbox(Mutex<Vec<Vec<u64>>>);

impl Inbox {
    fn count(&self) -> usize {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .map(Vec::len)
            .sum()
    }
}

impl ApplicationCallback for Inbox {
    fn on_msg_from_app(&self, msg: &Message, _: &SessionId) -> Result<(), MsgFromAppError> {
        let headline = msg.get_field(148).unwrap_or_default();
        let parsed = headline.strip_prefix('t').and_then(|rest| {
            let (thread, n) = rest.split_once(".n")?;
            Some((thread.parse::<usize>().ok()?, n.parse::<u64>().ok()?))
        });
        let (thread, n) = parsed.ok_or(MsgFromAppError::IncorrectTagValue { tag: 148 })?;
        let mut all = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        if all.len() <= thread {
            all.resize(thread + 1, Vec::new());
        }
        all[thread].push(n);
        Ok(())
    }
}

/// Sends `t<thread>.n<k>` until `stop`; returns the `k` whose send returned `Ok`.
fn send_until(thread: usize, stop: &AtomicBool) -> Vec<u64> {
    let id = ServerType::Sender.session_id();
    let mut accepted = Vec::new();
    let mut k = 0u64;
    while !stop.load(Ordering::Relaxed) {
        let news = build_news(&format!("t{thread}.n{k}"), &[]).expect("news builds");
        if send_to_target(news, &id).is_ok() {
            accepted.push(k);
        }
        k += 1;
    }
    accepted
}

fn file_store_settings(server_type: ServerType, port: u16, dir: &Path) -> SessionSettings {
    let mut settings = build_settings(server_type, port).expect("settings");
    let path = dir.to_str().expect("UTF-8 temp dir");
    settings
        .set(
            None,
            Dictionary::try_from_items(&[&FileStorePath(path)]).expect("store path"),
        )
        .expect("store path set");
    settings
}

/// Senders keep sending on file stores while both sides are logged out and in, stopped and
/// started, dropped and rebuilt. Every message whose send returned `Ok` must arrive once, in
/// order. (`Session::reset` is left out: it wipes the store by design, see the seqnum runbook.)
#[test]
#[ignore = "opt-in soak: QFRS_SOAK_SECS seconds of chaos, see the file header"]
fn chaos_delivers_every_accepted_message_once_in_order() -> Result<(), QuickFixError> {
    let _lock = serial();
    let port = find_available_port();
    let dir = env::temp_dir().join(format!("qfrs-soak-{}", std::process::id()));
    fs::remove_dir_all(&dir).ok();
    let settings_client = file_store_settings(ServerType::Sender, port, &dir.join("client"));
    let settings_server = file_store_settings(ServerType::Receiver, port, &dir.join("server"));
    let (client, server) = (NullFixApplication, Inbox::default());
    let log = LogFactory::try_new(&NullLogger)?;
    let app_client = Application::try_new(&client)?;
    let app_server = Application::try_new(&server)?;
    let store_client = FileMessageStoreFactory::try_new(&settings_client)?;
    let store_server = FileMessageStoreFactory::try_new(&settings_server)?;
    let kind = FixSocketServerKind::MultiThreaded;
    let new_acceptor =
        || Acceptor::try_new(&settings_server, &app_server, &store_server, &log, kind);
    let new_initiator =
        || Initiator::try_new(&settings_client, &app_client, &store_client, &log, kind);
    let mut acceptor = new_acceptor()?;
    let mut initiator = new_initiator()?;
    acceptor.start()?;
    initiator.start()?;

    let stop = Arc::new(AtomicBool::new(false));
    let senders: Vec<_> = (0..SENDER_THREADS)
        .map(|i| {
            let stop = Arc::clone(&stop);
            thread::spawn(move || send_until(i, &stop))
        })
        .collect();

    let end = Instant::now() + Duration::from_secs(env_or("QFRS_SOAK_SECS", 60));
    let mut round = 0u64;
    while Instant::now() < end {
        thread::sleep(Duration::from_millis(150 + round * 97 % 350));
        match round % 5 {
            0 => {
                let id = ServerType::Sender.session_id();
                // SAFETY: the initiator that owns this session lives on this thread and is not
                // dropped while the session handle is in use.
                let found = unsafe { Session::lookup(&id) };
                if let Ok(mut session) = found {
                    session.logout()?;
                    thread::sleep(Duration::from_millis(200));
                    session.logon()?;
                }
            }
            1 => {
                acceptor.stop()?;
                acceptor.start()?;
            }
            2 => {
                initiator.stop()?;
                initiator.start()?;
            }
            3 => {
                drop(initiator);
                initiator = new_initiator()?;
                initiator.start()?;
            }
            _ => {
                drop(acceptor);
                acceptor = new_acceptor()?;
                acceptor.start()?;
            }
        }
        round += 1;
    }
    stop.store(true, Ordering::Relaxed);
    let accepted: Vec<Vec<u64>> = senders
        .into_iter()
        .map(|h| h.join().expect("sender thread"))
        .collect();
    let total: usize = accepted.iter().map(Vec::len).sum();
    let settled = checker::wait_until(Duration::from_secs(60), || server.count() >= total);
    initiator.stop()?;
    acceptor.stop()?;

    println!(
        "{round} chaos rounds; {total} sends accepted; {} received; settled={settled}",
        server.count()
    );
    assert_delivered_once_in_order(&accepted, &server);
    fs::remove_dir_all(&dir).ok();
    Ok(())
}

fn assert_delivered_once_in_order(accepted: &[Vec<u64>], server: &Inbox) {
    let received = server.0.lock().unwrap_or_else(PoisonError::into_inner);
    for (i, sent) in accepted.iter().enumerate() {
        let got = received.get(i).map(Vec::as_slice).unwrap_or_default();
        assert!(
            got.windows(2).all(|w| w[0] < w[1]),
            "thread {i}: duplicate or out of order"
        );
        let lost = sent
            .iter()
            .filter(|k| got.binary_search(k).is_err())
            .count();
        assert_eq!(lost, 0, "thread {i}: {lost} accepted messages lost");
    }
}

/// Client side of the latency run: signals each echo it gets.
#[derive(Debug)]
struct Echoes(SyncSender<()>);

impl ApplicationCallback for Echoes {
    fn on_msg_from_app(&self, _: &Message, _: &SessionId) -> Result<(), MsgFromAppError> {
        // The receiver only goes away after the run.
        self.0.send(()).ok();
        Ok(())
    }
}

/// Server side of the latency run: echoes every message back from `on_msg_from_app`.
#[derive(Debug)]
struct Echo;

impl ApplicationCallback for Echo {
    fn on_msg_from_app(&self, _: &Message, session: &SessionId) -> Result<(), MsgFromAppError> {
        let echo = build_news("echo", &[]).expect("news builds");
        // A failed echo shows up as the client's 5 s timeout.
        send_to_target(echo, session).ok();
        Ok(())
    }
}

/// Round trip of one News through both sides, measured from the client; prints percentiles.
#[test]
#[ignore = "opt-in soak: QFRS_LATENCY_N round trips, see the file header"]
fn round_trip_latency_percentiles() -> Result<(), QuickFixError> {
    let _lock = serial();
    let port = find_available_port();
    let settings_client = build_settings(ServerType::Sender, port)?;
    let settings_server = build_settings(ServerType::Receiver, port)?;
    let (tx, rx) = sync_channel(1);
    let (client, server) = (Echoes(tx), Echo);
    let log = LogFactory::try_new(&NullLogger)?;
    let app_client = Application::try_new(&client)?;
    let app_server = Application::try_new(&server)?;
    let store = MemoryMessageStoreFactory::new();
    let kind = FixSocketServerKind::MultiThreaded;
    let mut acceptor = Acceptor::try_new(&settings_server, &app_server, &store, &log, kind)?;
    let mut initiator = Initiator::try_new(&settings_client, &app_client, &store, &log, kind)?;
    acceptor.start()?;
    initiator.start()?;
    assert!(checker::wait_until(Duration::from_secs(10), || {
        initiator.is_logged_on().unwrap_or(false) && acceptor.is_logged_on().unwrap_or(false)
    }));

    let n = usize::try_from(env_or("QFRS_LATENCY_N", 10_000)).expect("count fits usize");
    let warm_up = n / 10;
    let id = ServerType::Sender.session_id();
    let mut micros = Vec::with_capacity(n);
    for i in 0..n + warm_up {
        let started = Instant::now();
        send_to_target(build_news("ping", &[])?, &id)?;
        rx.recv_timeout(Duration::from_secs(5))
            .expect("echo within 5 s");
        if i >= warm_up {
            micros.push(started.elapsed().as_micros());
        }
    }
    initiator.stop()?;
    acceptor.stop()?;

    micros.sort_unstable();
    let at = |q: usize| micros[(micros.len() * q / 1000).min(micros.len() - 1)];
    println!(
        "round trip us over {n}: p50={} p90={} p99={} p99.9={} max={}",
        at(500),
        at(900),
        at(990),
        at(999),
        at(1000),
    );
    Ok(())
}
