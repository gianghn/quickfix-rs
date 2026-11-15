//! Opt-in soak runs, ported from the research harness's `stress` and `latency` modes (go-live
//! phase 11). Too slow for every CI run, so both are `#[ignore]`d; run them with
//!
//! ```text
//! QFRS_SOAK_SECS=300 cargo test -p quickfix --release --test test_soak -- --ignored --nocapture
//! ```
//!
//! `QFRS_SOAK_SECS` sets the chaos length (default 60), `QFRS_LATENCY_N` the round trips
//! (default 10000). The reset chaos (R-P1-7, go-live phase 25) takes `QFRS_RESET_RUNS`,
//! `QFRS_RESET_SECS`, `QFRS_SEED` and `QFRS_WATCHDOG_SECS` (see its test).

use std::{
    env, fs,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{sync_channel, SyncSender},
        Arc, Mutex, MutexGuard, OnceLock, PoisonError,
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
    // QFRS_SEND_GAP_US paces the sends; 0 (default) floods, which starves the session's own work.
    let gap = Duration::from_micros(env_or("QFRS_SEND_GAP_US", 0));
    let mut accepted = Vec::new();
    let mut k = 0u64;
    while !stop.load(Ordering::Relaxed) {
        let news = build_news(&format!("t{thread}.n{k}"), &[]).expect("news builds");
        if send_to_target(news, &id).is_ok() {
            accepted.push(k);
        }
        k += 1;
        if !gap.is_zero() {
            thread::sleep(gap);
        }
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

type ResetAcceptor<'a> = Acceptor<'a, Inbox, EventLog, FileMessageStoreFactory>;
type ResetInitiator<'a> = Initiator<'a, NullFixApplication, EventLog, FileMessageStoreFactory>;

/// The reset chaos actions, picked by the seeded generator; `RESET` is `Session::reset`.
const CHAOS_ACTIONS: [&str; 7] = [
    "initiator session logout + logon",
    "acceptor stop + start",
    "initiator stop + start",
    "initiator drop + rebuild",
    "acceptor drop + rebuild",
    "initiator session reset",
    "both stop + start",
];
const RESET: usize = 5;

/// xorshift64: the same seed gives the same chaos.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0 % n
    }
}

/// Aborts with every thread's stack when one action runs longer than its limit (R-P1-7).
/// Prints every thread's stack once when an action runs past `stall`, and aborts past `hang`
/// (R-P1-7); `run` records each action that ran past `stall`.
struct Watchdog {
    current: Arc<Mutex<Option<(&'static str, Instant)>>>,
    stall: Duration,
    stalls: Mutex<Vec<(&'static str, Duration)>>,
}

impl Watchdog {
    fn start(stall: Duration, hang: Duration) -> Self {
        let current = Arc::new(Mutex::new(None::<(&'static str, Instant)>));
        let watched = Arc::clone(&current);
        thread::spawn(move || {
            let mut dumped = None;
            loop {
                thread::sleep(Duration::from_millis(250));
                let action = *watched.lock().unwrap_or_else(PoisonError::into_inner);
                let Some((name, since)) = action else {
                    continue;
                };
                if since.elapsed() > hang {
                    dump_stacks(name, since.elapsed());
                    std::process::abort();
                }
                if since.elapsed() > stall && dumped != Some(since) {
                    dump_stacks(name, since.elapsed());
                    dumped = Some(since);
                }
            }
        });
        Self {
            current,
            stall,
            stalls: Mutex::default(),
        }
    }

    fn run<T>(&self, name: &'static str, action: impl FnOnce() -> T) -> T {
        let since = Instant::now();
        *self.current.lock().unwrap_or_else(PoisonError::into_inner) = Some((name, since));
        verbose(&format!("action: {name}"));
        let out = action();
        *self.current.lock().unwrap_or_else(PoisonError::into_inner) = None;
        if since.elapsed() > self.stall {
            let mut stalls = self.stalls.lock().unwrap_or_else(PoisonError::into_inner);
            stalls.push((name, since.elapsed()));
        }
        out
    }
}

/// With `QFRS_VERBOSE` set, prints `text` with the seconds since the first line.
fn verbose(text: &str) {
    static START: OnceLock<Instant> = OnceLock::new();
    if env::var_os("QFRS_VERBOSE").is_some() {
        let at = START.get_or_init(Instant::now).elapsed().as_secs_f64();
        eprintln!("[{at:>9.3}] {text}");
    }
}

/// Session events and admin messages for `QFRS_VERBOSE` (application messages are left out).
#[derive(Debug)]
struct EventLog;

impl EventLog {
    fn admin(direction: &str, id: Option<&SessionId>, msg: &str) {
        if !msg.contains("\u{1}35=B\u{1}") {
            let side = id
                .and_then(SessionId::get_sender_comp_id)
                .unwrap_or_default();
            verbose(&format!("{side} {direction} {}", msg.replace('\u{1}', "|")));
        }
    }
}

impl LogCallback for EventLog {
    fn on_incoming(&self, id: Option<&SessionId>, msg: &str) {
        Self::admin("in ", id, msg);
    }

    fn on_outgoing(&self, id: Option<&SessionId>, msg: &str) {
        Self::admin("out", id, msg);
    }

    fn on_event(&self, id: Option<&SessionId>, msg: &str) {
        Self::admin("event", id, &format!("\u{1}{msg}"));
    }
}

fn dump_stacks(action: &str, stuck: Duration) {
    eprintln!("WATCHDOG: '{action}' stuck for {stuck:?}; thread stacks follow");
    let pid = std::process::id().to_string();
    let (debugger, args): (&str, &[&str]) = if cfg!(target_os = "macos") {
        ("lldb", &["--batch", "-o", "thread backtrace all", "-p"])
    } else {
        ("gdb", &["-batch", "-ex", "thread apply all bt", "-p"])
    };
    let mut child = match std::process::Command::new(debugger)
        .args(args)
        .arg(&pid)
        .stdin(std::process::Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(err) => return eprintln!("WATCHDOG: no stacks, {debugger} did not run: {err}"),
    };
    // A debugger that never returns (seen once with lldb) must not keep the watchdog from its abort.
    let deadline = Instant::now() + Duration::from_secs(30);
    while matches!(child.try_wait(), Ok(None)) {
        if Instant::now() > deadline {
            eprintln!("WATCHDOG: {debugger} still attached after 30 s, killed");
            child.kill().ok();
            child.wait().ok();
            return;
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn started<C: ConnectionHandler>(mut handler: C) -> Result<C, QuickFixError> {
    handler.start()?;
    Ok(handler)
}

/// Every action but drop + rebuild, which the run loop does: the old object must be dropped
/// before the new one is built, or its drop unregisters the new one's session (a segfault in
/// `Initiator::connect`, found by this test).
fn chaos_step(
    action: usize,
    acceptor: &mut ResetAcceptor<'_>,
    initiator: &mut ResetInitiator<'_>,
) -> Result<(), QuickFixError> {
    let id = ServerType::Sender.session_id();
    match action {
        0 | RESET => {
            // SAFETY: the initiator that owns this session lives on this thread and is not
            // dropped while the session handle is in use.
            if let Ok(mut session) = unsafe { Session::lookup(&id) } {
                if action == RESET {
                    return session.reset();
                }
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
        _ => {
            initiator.stop()?;
            acceptor.stop()?;
            acceptor.start()?;
            initiator.start()?;
        }
    }
    Ok(())
}

/// One seeded run; returns (rounds, resets, sends accepted, messages received).
fn reset_chaos_run(
    seed: u64,
    secs: u64,
    watchdog: &Watchdog,
) -> Result<(u64, u64, usize, usize), QuickFixError> {
    let port = find_available_port();
    let dir = env::temp_dir().join(format!("qfrs-reset-{}-{seed}", std::process::id()));
    fs::remove_dir_all(&dir).ok();
    let settings_client = file_store_settings(ServerType::Sender, port, &dir.join("client"));
    let settings_server = file_store_settings(ServerType::Receiver, port, &dir.join("server"));
    let (client, server) = (NullFixApplication, Inbox::default());
    let log = LogFactory::try_new(&EventLog)?;
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
    verbose(&format!("seed {seed} started"));
    let mut rng = Rng::new(seed);
    let (mut rounds, mut resets) = (0, 0);
    let end = Instant::now() + Duration::from_secs(secs);
    while Instant::now() < end {
        thread::sleep(Duration::from_millis(150 + rng.below(350)));
        // Half the actions are resets, so a short run still exercises it.
        let pick = if rng.below(2) == 0 {
            RESET as u64
        } else {
            rng.below(CHAOS_ACTIONS.len() as u64)
        };
        let action = usize::try_from(pick).unwrap_or(RESET);
        resets += u64::from(action == RESET);
        let name = CHAOS_ACTIONS[action];
        let failed =
            |err: &QuickFixError| eprintln!("seed {seed}, round {rounds}: '{name}' failed: {err}");
        match action {
            3 => {
                watchdog.run(name, || drop(initiator));
                initiator = watchdog
                    .run(name, || started(new_initiator()?))
                    .inspect_err(failed)?;
            }
            4 => {
                watchdog.run(name, || drop(acceptor));
                acceptor = watchdog
                    .run(name, || started(new_acceptor()?))
                    .inspect_err(failed)?;
            }
            _ => watchdog
                .run(name, || chaos_step(action, &mut acceptor, &mut initiator))
                .inspect_err(failed)?,
        }
        rounds += 1;
    }
    stop.store(true, Ordering::Relaxed);
    let accepted: usize = watchdog.run("join senders", || {
        senders
            .into_iter()
            .map(|h| h.join().expect("sender thread").len())
            .sum()
    });
    watchdog
        .run("final stop", || initiator.stop().and(acceptor.stop()))
        .inspect_err(|err| eprintln!("seed {seed}: 'final stop' failed: {err}"))?;
    watchdog.run("final drop", || drop((initiator, acceptor)));
    fs::remove_dir_all(&dir).ok();
    Ok((rounds, resets, accepted, server.count()))
}

/// R-P1-7: seeded chaos runs with `Session::reset` among the actions. A reset wipes the store by
/// design, so delivery is not checked here (the test above checks it without reset); the run
/// fails if any action takes longer than `QFRS_WATCHDOG_SECS` (default 10), and the watchdog
/// prints every thread's stack before it aborts. `QFRS_RESET_RUNS` runs (default 30) of
/// `QFRS_RESET_SECS` each (default 10), seeds from `QFRS_SEED` (default 1).
#[test]
#[ignore = "opt-in soak: QFRS_RESET_RUNS seeded reset chaos runs, see the file header"]
fn chaos_with_reset_never_stalls() -> Result<(), QuickFixError> {
    let _lock = serial();
    let watchdog = Watchdog::start(
        Duration::from_secs(env_or("QFRS_WATCHDOG_SECS", 10)),
        Duration::from_secs(env_or("QFRS_HANG_SECS", 60)),
    );
    let first = env_or("QFRS_SEED", 1);
    for seed in first..first + env_or("QFRS_RESET_RUNS", 30) {
        let (rounds, resets, accepted, received) =
            reset_chaos_run(seed, env_or("QFRS_RESET_SECS", 10), &watchdog)?;
        println!(
            "seed {seed}: {rounds} chaos rounds, {resets} resets; {accepted} sends accepted, \
             {received} received"
        );
    }
    let stalls = watchdog
        .stalls
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    println!("stalls over the limit: {stalls:?}");
    Ok(())
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
