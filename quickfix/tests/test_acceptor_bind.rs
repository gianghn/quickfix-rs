//! `SocketAcceptHost` binds an acceptor to one IPv4 address (KRX fork; gateway review B6).

use std::net::Ipv4Addr;
use std::time::Duration;

use bind_support::*;
use quickfix::*;

mod bind_support;

const LOGON_WAIT: Duration = Duration::from_secs(10);

fn start_error(gateway: &mut Gateway) -> Option<QuickFixError> {
    let error = gateway.start().err();
    if error.is_none() {
        gateway.stop().expect("stop");
    }
    error
}

/// Starts `gateway`, then a broker connecting to 127.0.0.1; whether both sides log on.
fn broker_logs_on(gateway: &mut Gateway, port: u16) -> bool {
    gateway.start().expect("gateway starts");
    let mut broker: Broker = broker("BROKER", "127.0.0.1", port).expect("broker");
    broker.start().expect("broker starts");
    let logged_on = wait_until(LOGON_WAIT, || {
        gateway.is_logged_on().unwrap_or(false) && broker.is_logged_on().unwrap_or(false)
    });
    broker.stop().expect("broker stops");
    gateway.stop().expect("gateway stops");
    logged_on
}

#[test]
fn acceptor_start_logs_a_broker_on_when_bound_to_its_address() {
    let _lock = ENGINE_LOCK.lock().expect("ENGINE_LOCK poisoned");
    let port = free_port();
    let mut gateway = gateway(port, &[("BROKER", "127.0.0.1")]).expect("gateway");
    assert!(broker_logs_on(&mut gateway, port));
}

#[test]
fn acceptor_start_logs_a_broker_on_when_host_is_empty() {
    let _lock = ENGINE_LOCK.lock().expect("ENGINE_LOCK poisoned");
    let port = free_port();
    let mut gateway = gateway(port, &[("BROKER", "")]).expect("gateway");
    assert!(broker_logs_on(&mut gateway, port));
}

#[test]
fn acceptor_start_refuses_other_addresses_when_bound_to_loopback() {
    let Some(other) = other_local_address() else {
        // macOS with no network: no second local address to try.
        return;
    };
    let _lock = ENGINE_LOCK.lock().expect("ENGINE_LOCK poisoned");
    let port = free_port();
    let mut gateway = gateway(port, &[("BROKER", "127.0.0.1")]).expect("gateway");
    gateway.start().expect("gateway starts");
    let through_other = connects(other, port);
    let through_loopback = connects(Ipv4Addr::LOCALHOST, port);
    gateway.stop().expect("gateway stops");
    assert!(!through_other, "{other}:{port} must be refused");
    assert!(through_loopback);
}

#[test]
fn acceptor_start_returns_config_error_when_host_is_not_numeric_ipv4() {
    let _lock = ENGINE_LOCK.lock().expect("ENGINE_LOCK poisoned");
    for host in ["localhost", "::1", "10.0.0", "256.1.1.1", "x"] {
        let mut gateway = gateway(free_port(), &[("BROKER", host)]).expect("gateway");
        match start_error(&mut gateway) {
            Some(QuickFixError::ConfigError(text)) => assert_eq!(
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
    let mut gateway = gateway(port, &sessions).expect("gateway");
    match start_error(&mut gateway) {
        Some(QuickFixError::ConfigError(text)) => assert!(
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
    let mut gateway = gateway(port, &[("BROKER", "192.0.2.1")]).expect("gateway");
    match start_error(&mut gateway) {
        Some(QuickFixError::RuntimeError(text)) => assert!(
            text.contains(&format!(
                "Unable to create, bind, or listen to port {port} on 192.0.2.1"
            )),
            "{text}"
        ),
        other => panic!("want RuntimeError, got {other:?}"),
    }
}
