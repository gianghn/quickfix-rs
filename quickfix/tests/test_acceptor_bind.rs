//! `SocketAcceptHost` binds an acceptor to one IPv4 address (KRX fork; gateway review B6).

use std::net::Ipv4Addr;
use std::time::Duration;

use bind_support::*;
use quickfix::*;

mod bind_support;

const LOGON_WAIT: Duration = Duration::from_secs(10);

/// Starts a gateway with `sessions` on `port`, then a broker connecting to 127.0.0.1; whether
/// both sides log on.
fn broker_logs_on(host: &str) -> bool {
    let port = free_port();
    let gateway = start_gateway(port, &[("BROKER", host)]).expect("gateway starts");
    let broker = start_broker("BROKER", "127.0.0.1", port).expect("broker starts");
    wait_until(LOGON_WAIT, || {
        gateway.is_logged_on() && broker.is_logged_on()
    })
}

#[test]
fn acceptor_start_logs_a_broker_on_when_bound_to_its_address() {
    let _lock = ENGINE_LOCK.lock().expect("ENGINE_LOCK poisoned");
    assert!(broker_logs_on("127.0.0.1"));
}

#[test]
fn acceptor_start_logs_a_broker_on_when_host_is_empty() {
    let _lock = ENGINE_LOCK.lock().expect("ENGINE_LOCK poisoned");
    assert!(broker_logs_on(""));
}

#[test]
fn acceptor_start_refuses_other_addresses_when_bound_to_loopback() {
    let Some(other) = other_local_address() else {
        // macOS with no network: no second local address to try.
        return;
    };
    let _lock = ENGINE_LOCK.lock().expect("ENGINE_LOCK poisoned");
    let port = free_port();
    let _gateway = start_gateway(port, &[("BROKER", "127.0.0.1")]).expect("gateway starts");
    assert!(!connects(other, port), "{other}:{port} must be refused");
    assert!(connects(Ipv4Addr::LOCALHOST, port));
}

#[test]
fn acceptor_start_returns_config_error_when_host_is_not_numeric_ipv4() {
    let _lock = ENGINE_LOCK.lock().expect("ENGINE_LOCK poisoned");
    for host in ["localhost", "::1", "10.0.0", "256.1.1.1", "x"] {
        match start_gateway(free_port(), &[("BROKER", host)]) {
            Err(QuickFixError::ConfigError(text)) => assert_eq!(
                text,
                format!(
                    "Configuration failed: SocketAcceptHost must be a numeric IPv4 address: \
                     \"{host}\""
                )
            ),
            other => panic!("{host}: want ConfigError, got {other:?}"),
        }
    }
}

#[test]
fn acceptor_start_returns_config_error_when_one_port_has_two_hosts() {
    let _lock = ENGINE_LOCK.lock().expect("ENGINE_LOCK poisoned");
    let port = free_port();
    let sessions = [("BROKER", "127.0.0.1"), ("OTHER", "127.0.0.2")];
    match start_gateway(port, &sessions) {
        Err(QuickFixError::ConfigError(text)) => assert!(
            text.contains(&format!(
                "SocketAcceptPort {port} has two SocketAcceptHost values"
            )),
            "{text}"
        ),
        other => panic!("want ConfigError, got {other:?}"),
    }
}

#[test]
fn acceptor_start_returns_runtime_error_naming_port_and_host_when_address_is_not_local() {
    let _lock = ENGINE_LOCK.lock().expect("ENGINE_LOCK poisoned");
    let port = free_port();
    // 192.0.2.1 is TEST-NET-1: never an address of this host.
    match start_gateway(port, &[("BROKER", "192.0.2.1")]) {
        Err(QuickFixError::RuntimeError(text)) => assert!(
            text.contains(&format!(
                "Unable to create, bind, or listen to port {port} on 192.0.2.1"
            )),
            "{text}"
        ),
        other => panic!("want RuntimeError, got {other:?}"),
    }
}
