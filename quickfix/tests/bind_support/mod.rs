//! An acceptor bound to one address with `SocketAcceptHost` (KRX fork; gateway review B6), shared
//! by `test_acceptor_bind.rs` and the `acceptor_bind` cucumber suite.

use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

use quickfix::{dictionary_item::*, *};

/// QuickFIX keeps its sessions in one process-wide registry: one engine pair at a time.
pub static ENGINE_LOCK: Mutex<()> = Mutex::new(());

/// The application of every engine here: it does nothing.
#[derive(Debug)]
pub struct Quiet;
impl ApplicationCallback for Quiet {}

static QUIET: Quiet = Quiet;
static NO_LOG: NullLogger = NullLogger;

/// What an engine borrows for its whole life; leaked, so the engine can live in a test's state.
struct Parts {
    application: Application<'static, Quiet>,
    store: MemoryMessageStoreFactory,
    log: LogFactory<'static, NullLogger>,
}

fn leaked_parts() -> Result<&'static Parts, QuickFixError> {
    Ok(Box::leak(Box::new(Parts {
        application: Application::try_new(&QUIET)?,
        store: MemoryMessageStoreFactory::new(),
        log: LogFactory::try_new(&NO_LOG)?,
    })))
}

/// The gateway side: a threaded acceptor, the kind the gateway runs.
pub type Gateway = Acceptor<'static, Quiet, NullLogger, MemoryMessageStoreFactory>;
/// The broker side: a threaded initiator.
pub type Broker = Initiator<'static, Quiet, NullLogger, MemoryMessageStoreFactory>;

/// The gateway's session for the broker `target`.
pub fn gateway_session(target: &str) -> SessionId {
    SessionId::try_new("FIX.4.4", "GATEWAY", target, "").expect("valid session ID")
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

/// A gateway with one session per `(broker, host)` on `port`, each bound to its host.
///
/// # Errors
///
/// Whatever QuickFIX refuses while building the acceptor.
pub fn gateway(port: u16, sessions: &[(&str, &str)]) -> Result<Gateway, QuickFixError> {
    let mut settings = SessionSettings::new();
    settings.set(
        None,
        Dictionary::try_from_items(&[&ConnectionType::Acceptor])?,
    )?;
    for (broker, host) in sessions {
        let mut items = session_items(port)?;
        SocketAcceptHost(host).apply_param(&mut items)?;
        settings.set(Some(&gateway_session(broker)), items)?;
    }
    let parts = leaked_parts()?;
    Acceptor::try_new(
        &settings,
        &parts.application,
        &parts.store,
        &parts.log,
        FixSocketServerKind::MultiThreaded,
    )
}

/// The broker `name`, connecting to `address:port`.
///
/// # Errors
///
/// Whatever QuickFIX refuses while building the initiator.
pub fn broker(name: &str, address: &str, port: u16) -> Result<Broker, QuickFixError> {
    let mut settings = SessionSettings::new();
    settings.set(
        None,
        Dictionary::try_from_items(&[&ConnectionType::Initiator, &ReconnectInterval(1)])?,
    )?;
    let mut items = session_items(port)?;
    SocketConnectHost(address).apply_param(&mut items)?;
    let session = SessionId::try_new("FIX.4.4", name, "GATEWAY", "")?;
    settings.set(Some(&session), items)?;
    let parts = leaked_parts()?;
    Initiator::try_new(
        &settings,
        &parts.application,
        &parts.store,
        &parts.log,
        FixSocketServerKind::MultiThreaded,
    )
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
