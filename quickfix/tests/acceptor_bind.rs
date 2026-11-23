//! Scenarios of `tests/features/acceptor_bind.feature`: `SocketAcceptHost` (KRX fork; gateway
//! review B6), run by cucumber-rs.

use std::net::Ipv4Addr;
use std::time::Duration;

use bind_support::*;
use cucumber::{given, then, when, World};
use quickfix::*;

mod bind_support;

const LOGON_WAIT: Duration = Duration::from_secs(10);
const BROKER: &str = "BROKER";
/// Needs a second IPv4 address on this host: always on Linux (127.0.0.2), only with a network
/// elsewhere (macOS answers only 127.0.0.1 unless aliased).
const NEEDS_SECOND_ADDRESS: &str = "Connection through another address never reaches the session";

#[derive(Debug, Default, World)]
struct BindWorld {
    port: u16,
    gateway: Option<Gateway>,
    broker: Option<Broker>,
    connected: Option<bool>,
    start_error: Option<QuickFixError>,
}

impl BindWorld {
    fn gateway_listening_on(&mut self, host: &str) {
        self.port = free_port();
        self.gateway = Some(gateway(self.port, &[(BROKER, host)]).expect("gateway"));
    }

    fn started_gateway(&mut self) -> &mut Gateway {
        let gateway = self.gateway.as_mut().expect("a gateway");
        gateway.start().expect("gateway starts");
        gateway
    }
}

#[given("a broker port that listens only on the loopback address")]
fn port_on_loopback(world: &mut BindWorld) {
    world.gateway_listening_on("127.0.0.1");
    world.started_gateway();
}

#[given("a broker port whose listening address is a host name")]
fn port_on_host_name(world: &mut BindWorld) {
    world.gateway_listening_on("localhost");
}

#[when("the broker connects through the loopback address")]
fn broker_connects(world: &mut BindWorld) {
    let mut broker = broker(BROKER, "127.0.0.1", world.port).expect("broker");
    broker.start().expect("broker starts");
    world.broker = Some(broker);
}

#[when("someone connects through another address of the host")]
fn connects_through_other(world: &mut BindWorld) {
    let other = other_local_address().expect("a second address (filtered in main otherwise)");
    world.connected = Some(connects(other, world.port));
}

#[when("the engine starts")]
fn engine_starts(world: &mut BindWorld) {
    let gateway = world.gateway.as_mut().expect("a gateway");
    world.start_error = gateway.start().err();
}

#[then("the broker's session is logged on")]
fn session_logged_on(world: &mut BindWorld) {
    let gateway = world.gateway.as_ref().expect("a gateway");
    assert!(wait_until(LOGON_WAIT, || gateway
        .is_logged_on()
        .unwrap_or(false)));
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
            .max_concurrent_scenarios(1)
            .fail_on_skipped()
            .filter_run_and_exit("tests/features", move |_, _, scenario| {
                second_address || scenario.name != NEEDS_SECOND_ADDRESS
            }),
    );
}
