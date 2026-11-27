//! Scenarios of `tests/features`, run by cucumber-rs: `acceptor_bind.feature` (`SocketAcceptHost`;
//! KRX fork, gateway review B6) and `allowed_addresses.feature` (`AllowedRemoteAddresses`; the B6
//! follow-up). One binary, so every helper of `bind_support` is used.

use std::net::Ipv4Addr;
use std::time::Duration;

use bind_support::*;
use cucumber::{given, then, when, World};
use quickfix::*;

mod bind_support;

const LOGON_WAIT: Duration = Duration::from_secs(10);
/// The broker redials every second: three tries.
const REFUSED_WAIT: Duration = Duration::from_secs(3);
const BROKER: &str = "BROKER";
/// Needs a second IPv4 address on this host: always on Linux (127.0.0.2), only with a network
/// elsewhere (macOS answers only 127.0.0.1 unless aliased).
const NEEDS_SECOND_ADDRESS: &str = "Connection through another address never reaches the session";

/// Fields drop in order: the broker stops before the gateway.
#[derive(Debug, Default, World)]
struct BindWorld {
    port: u16,
    host: String,
    /// `AllowedRemoteAddresses` of the broker's session; when set, the port is not bound.
    allowed: Option<String>,
    broker: Option<Engine>,
    gateway: Option<Engine>,
    connected: Option<bool>,
    start_error: Option<QuickFixError>,
}

#[given("a broker port that listens only on the loopback address")]
fn port_on_loopback(world: &mut BindWorld) {
    world.port = free_port();
    let gateway = start_gateway(world.port, &[(BROKER, "127.0.0.1")]).expect("gateway starts");
    world.gateway = Some(gateway);
}

#[given("a broker port whose listening address is a host name")]
fn port_on_host_name(world: &mut BindWorld) {
    world.port = free_port();
    world.host = "localhost".to_owned();
}

#[given("a broker session that may log on from the loopback address")]
fn allowed_from_loopback(world: &mut BindWorld) {
    // TEST-NET-1 first: the list is searched, not its first entry taken.
    allow(world, "192.0.2.10, 127.0.0.1");
    world.gateway = Some(start(world).expect("gateway starts"));
}

#[given("a broker session that may log on only from another host")]
fn allowed_from_elsewhere(world: &mut BindWorld) {
    allow(world, "192.0.2.10");
    world.gateway = Some(start(world).expect("gateway starts"));
}

#[given("a broker session whose address list holds a host name")]
fn allowed_host_name(world: &mut BindWorld) {
    allow(world, "127.0.0.1,localhost");
}

fn allow(world: &mut BindWorld, allowed: &str) {
    world.port = free_port();
    world.allowed = Some(allowed.to_owned());
}

/// The gateway of the scenario: bound to its host, or limited to its allowed addresses.
fn start(world: &BindWorld) -> Result<Engine, QuickFixError> {
    match &world.allowed {
        Some(allowed) => start_gateway_allowing(world.port, BROKER, allowed),
        None => start_gateway(world.port, &[(BROKER, &world.host)]),
    }
}

#[when("the broker connects through the loopback address")]
fn broker_connects(world: &mut BindWorld) {
    let broker = start_broker(BROKER, "127.0.0.1", world.port).expect("broker starts");
    world.broker = Some(broker);
}

#[when("someone connects through another address of the host")]
fn connects_through_other(world: &mut BindWorld) {
    let other = other_local_address().expect("a second address (filtered in main otherwise)");
    world.connected = Some(connects(other, world.port));
}

#[when("the engine starts")]
fn engine_starts(world: &mut BindWorld) {
    match start(world) {
        Ok(gateway) => world.gateway = Some(gateway),
        Err(error) => world.start_error = Some(error),
    }
}

#[then("the broker's session is logged on")]
fn session_logged_on(world: &mut BindWorld) {
    let gateway = world.gateway.as_ref().expect("a gateway");
    assert!(wait_until(LOGON_WAIT, || gateway.is_logged_on()));
}

#[then("the broker's session is not logged on")]
fn session_not_logged_on(world: &mut BindWorld) {
    let gateway = world.gateway.as_ref().expect("a gateway");
    assert!(!wait_until(REFUSED_WAIT, || gateway.is_logged_on()));
}

#[then("it stops with a configuration error that names the address list setting")]
fn allowed_config_error(world: &mut BindWorld) {
    match world.start_error.take() {
        Some(QuickFixError::ConfigError(text)) => {
            assert!(text.contains("AllowedRemoteAddresses"), "{text}");
        }
        other => panic!("want ConfigError, got {other:?}"),
    }
}

#[then("the connection is refused")]
fn connection_refused(world: &mut BindWorld) {
    assert_eq!(world.connected, Some(false));
    // The port itself is up: the loopback address still connects.
    assert!(connects(Ipv4Addr::LOCALHOST, world.port));
}

#[then("it stops with a configuration error that names the listening address setting")]
fn config_error(world: &mut BindWorld) {
    match world.start_error.take() {
        Some(QuickFixError::ConfigError(text)) => {
            assert!(text.contains("SocketAcceptHost"), "{text}");
        }
        other => panic!("want ConfigError, got {other:?}"),
    }
}

fn main() {
    let _lock = ENGINE_LOCK.lock().expect("ENGINE_LOCK poisoned");
    let second_address = other_local_address().is_some();
    if !second_address {
        eprintln!(
            "skipped \"{NEEDS_SECOND_ADDRESS}\": this host has no IPv4 address besides 127.0.0.1"
        );
    }
    futures::executor::block_on(
        BindWorld::cucumber()
            // No command line: `cargo test --workspace -- --exact --skip …` (scripts/san.sh)
            // passes libtest's arguments to every test binary, and cucumber's parser refuses them.
            .with_cli(cucumber::cli::Opts::<_, _, _, cucumber::cli::Empty>::default())
            .max_concurrent_scenarios(1)
            .fail_on_skipped()
            .filter_run_and_exit("tests/features", move |_, _, scenario| {
                second_address || scenario.name != NEEDS_SECOND_ADDRESS
            }),
    );
}
