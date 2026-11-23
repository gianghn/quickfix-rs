//! An acceptor bound to one address with `SocketAcceptHost` (KRX fork; gateway review B6), shared
//! by `test_acceptor_bind.rs` and the `acceptor_bind` cucumber suite.

use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use quickfix::{dictionary_item::*, *};

/// QuickFIX keeps its sessions in one process-wide registry: one engine pair at a time.
pub static ENGINE_LOCK: Mutex<()> = Mutex::new(());

/// The application of every engine here: it does nothing.
struct Quiet;
impl ApplicationCallback for Quiet {}

static QUIET: Quiet = Quiet;
static NO_LOG: NullLogger = NullLogger;

/// Builds an engine from plain data `D` and the parts it borrows.
type Build<D> = for<'a> fn(
    &D,
    &'a Application<'static, Quiet>,
    &'a MemoryMessageStoreFactory,
    &'a LogFactory<'static, NullLogger>,
) -> Result<Box<dyn ConnectionHandler + 'a>, QuickFixError>;

/// An engine (acceptor or initiator) on its own thread, which owns everything the engine borrows
/// (no leak, and QuickFIX's handles need not be `Send`). Dropping it stops the engine.
#[derive(Debug)]
pub struct Engine {
    stop: Option<mpsc::Sender<()>>,
    logged_on: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Engine {
    /// Whether the engine has a session logged on.
    pub fn is_logged_on(&self) -> bool {
        self.logged_on.load(Ordering::SeqCst)
    }

    /// Builds and starts the engine on a new thread; returns once `start()` returned.
    fn spawn<D: Send + 'static>(data: D, build: Build<D>) -> Result<Self, QuickFixError> {
        let (started_tx, started_rx) = mpsc::channel();
        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        let logged_on = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&logged_on);
        let thread = thread::spawn(move || {
            let result = run(&data, build, &started_tx, &stop_rx, &flag);
            if let Err(error) = result {
                let _ = started_tx.send(Err(error));
            }
        });
        let engine = Self {
            stop: Some(stop_tx),
            logged_on,
            thread: Some(thread),
        };
        let started = started_rx
            .recv()
            .unwrap_or_else(|_| Err(QuickFixError::InvalidArgument("engine thread ended".into())));
        started.map(|()| engine)
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        drop(self.stop.take());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// The engine thread: build, start, report, keep `logged_on` current until told to stop.
fn run<D>(
    data: &D,
    build: Build<D>,
    started: &mpsc::Sender<Result<(), QuickFixError>>,
    stop: &mpsc::Receiver<()>,
    logged_on: &AtomicBool,
) -> Result<(), QuickFixError> {
    let application = Application::try_new(&QUIET)?;
    let store = MemoryMessageStoreFactory::new();
    let log = LogFactory::try_new(&NO_LOG)?;
    let mut engine = build(data, &application, &store, &log)?;
    engine.start()?;
    let _ = started.send(Ok(()));
    while let Err(mpsc::RecvTimeoutError::Timeout) = stop.recv_timeout(Duration::from_millis(20)) {
        logged_on.store(engine.is_logged_on().unwrap_or(false), Ordering::SeqCst);
    }
    engine.stop()
}

/// A port nothing listens on now.
pub fn free_port() -> u16 {
    TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .and_then(|listener| listener.local_addr())
        .map(|address| address.port())
        .expect("an ephemeral port")
}

fn session_items(port: u16) -> Result<Dictionary, QuickFixError> {
    Dictionary::try_from_items(&[
        &StartTime("00:00:00"),
        &EndTime("23:59:59"),
        &HeartBtInt(20),
        &DataDictionary("../quickfix-ffi/libquickfix/spec/FIX44.xml"),
        &SocketAcceptPort(port),
        &SocketConnectPort(port),
    ])
}

/// Starts a gateway (a threaded acceptor, the kind the gateway runs) with one session per
/// `(broker, host)` on `port`, each bound to its host.
///
/// # Errors
///
/// What QuickFIX refuses while building or starting the acceptor.
pub fn start_gateway(port: u16, sessions: &[(&str, &str)]) -> Result<Engine, QuickFixError> {
    let sessions: Vec<(String, String)> = sessions
        .iter()
        .map(|(broker, host)| ((*broker).to_owned(), (*host).to_owned()))
        .collect();
    Engine::spawn(
        (port, sessions),
        |(port, sessions), application, store, log| {
            let mut settings = SessionSettings::new();
            settings.set(
                None,
                Dictionary::try_from_items(&[&ConnectionType::Acceptor])?,
            )?;
            for (broker, host) in sessions {
                let mut items = session_items(*port)?;
                SocketAcceptHost(host).apply_param(&mut items)?;
                let session = SessionId::try_new("FIX.4.4", "GATEWAY", broker, "")?;
                settings.set(Some(&session), items)?;
            }
            let acceptor = Acceptor::try_new(
                &settings,
                application,
                store,
                log,
                FixSocketServerKind::MultiThreaded,
            )?;
            Ok(Box::new(acceptor))
        },
    )
}

/// Starts the broker `name` (a threaded initiator), connecting to `address:port`.
///
/// # Errors
///
/// What QuickFIX refuses while building or starting the initiator.
pub fn start_broker(name: &str, address: &str, port: u16) -> Result<Engine, QuickFixError> {
    let data = (name.to_owned(), address.to_owned(), port);
    Engine::spawn(data, |(name, address, port), application, store, log| {
        let mut settings = SessionSettings::new();
        settings.set(
            None,
            Dictionary::try_from_items(&[&ConnectionType::Initiator, &ReconnectInterval(1)])?,
        )?;
        let mut items = session_items(*port)?;
        SocketConnectHost(address).apply_param(&mut items)?;
        let session = SessionId::try_new("FIX.4.4", name, "GATEWAY", "")?;
        settings.set(Some(&session), items)?;
        let initiator = Initiator::try_new(
            &settings,
            application,
            store,
            log,
            FixSocketServerKind::MultiThreaded,
        )?;
        Ok(Box::new(initiator))
    })
}

/// Polls `done` every 20 ms until it holds or `timeout` passes; returns its last answer.
pub fn wait_until(timeout: Duration, mut done: impl FnMut() -> bool) -> bool {
    let start = Instant::now();
    while !done() {
        if start.elapsed() > timeout {
            return false;
        }
        thread::sleep(Duration::from_millis(20));
    }
    true
}

/// An IPv4 address of this host other than 127.0.0.1, or `None` when it has none.
///
/// The source address the kernel picks for a route out (a UDP `connect` sends nothing; 192.0.2.1
/// is TEST-NET-1); failing that, on Linux, 127.0.0.2, which is local there (macOS answers only
/// 127.0.0.1 unless aliased).
pub fn other_local_address() -> Option<Ipv4Addr> {
    let routed = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))
        .and_then(|socket| {
            socket.connect((Ipv4Addr::new(192, 0, 2, 1), 9))?;
            socket.local_addr()
        })
        .ok()
        .and_then(|address| match address {
            SocketAddr::V4(v4) if !v4.ip().is_loopback() && !v4.ip().is_unspecified() => {
                Some(*v4.ip())
            }
            _ => None,
        });
    if routed.is_some() || !cfg!(target_os = "linux") {
        return routed;
    }
    Some(Ipv4Addr::new(127, 0, 0, 2))
}

/// Whether a TCP connection to `address:port` is accepted within a second.
pub fn connects(address: Ipv4Addr, port: u16) -> bool {
    TcpStream::connect_timeout(&SocketAddr::from((address, port)), Duration::from_secs(1)).is_ok()
}
