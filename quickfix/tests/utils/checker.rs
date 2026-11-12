use std::{
    sync::Mutex,
    thread,
    time::{Duration, Instant},
};

use quickfix::*;

use super::{build_news, build_settings, ServerType};

pub static GLOBAL_LOCK: Mutex<u8> = Mutex::new(0);

/// Polls `done` until it holds or `timeout` passes; returns its last answer.
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

/// Check full FIX application lifecycle:
/// 1. Start and connect Acceptor <-> Initiator.
/// 2. Send message in both direction.
/// 3. Disconnect the 2 components.
///
/// WARNING: There is a lot of global variables in QuickFix C++. So this function
///          cannot be instantiate using multiple thread.
///          This is why there is a global lock in the application.
///          It also make port re-use easier.
pub fn run(
    communication_port: u16,
    logger: impl LogCallback + 'static,
    sender: impl ApplicationCallback + 'static,
    message_store_factory_sender: impl FfiMessageStoreFactory + 'static,
    receiver: impl ApplicationCallback + 'static,
    message_store_factory_receiver: impl FfiMessageStoreFactory + 'static,
) -> Result<(), QuickFixError> {
    let _lock = GLOBAL_LOCK.lock().expect("GLOBAL_LOCK poisoned");

    // Init settings with same server port.
    let settings_sender = build_settings(ServerType::Sender, communication_port)?;
    let settings_receiver = build_settings(ServerType::Receiver, communication_port)?;

    let log_factory = LogFactory::try_new(&logger)?;

    let app_sender = Application::try_new(&sender)?;
    let app_receiver = Application::try_new(&receiver)?;

    // Init socket acceptor / initiator.
    let mut socket_sender = Initiator::try_new(
        &settings_sender,
        &app_sender,
        &message_store_factory_sender,
        &log_factory,
        FixSocketServerKind::default(),
    )?;
    let mut socket_receiver = Acceptor::try_new(
        &settings_receiver,
        &app_receiver,
        &message_store_factory_receiver,
        &log_factory,
        FixSocketServerKind::default(),
    )?;

    // Start the app
    socket_receiver.start()?;
    socket_sender.start()?;

    // Wait until both sides are logged on, not either (R-P0-8: sending before the receiver
    // was up raced its logon).
    let logged_on = wait_until(Duration::from_secs(10), || {
        socket_sender.is_logged_on().unwrap_or(false)
            && socket_receiver.is_logged_on().unwrap_or(false)
    });
    assert!(logged_on, "both sides logged on");

    // Send message in both direction
    let news = build_news("Hello", &[])?;
    send_to_target(news, &ServerType::Sender.session_id())?;
    thread::sleep(Duration::from_millis(50));

    let news = build_news("Anyone here ?", &["Yeah"])?;
    send_to_target(news, &ServerType::Receiver.session_id())?;
    thread::sleep(Duration::from_millis(50));

    // Stop everything
    socket_receiver.stop()?;
    socket_sender.stop()?;

    Ok(())
}
