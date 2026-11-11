//! What reaches the counterparty when an application or log callback fails (go-live phase 3).
//!
//! - R-P0-1: a panic in any callback aborts the process. It used to be reported to QuickFIX as
//!   success, so an inbound message was lost (its seqnum consumed) and an outbound one was sent.
//! - R-P0-5: every `MsgFromAppError` reaches the peer as the reject QuickFIX defines for it;
//!   `UnsupportedMessageType` used to be dropped without a reply.
#![cfg(unix)]

use std::{
    env, fs,
    os::unix::process::ExitStatusExt,
    path::{Path, PathBuf},
    process::Command,
    sync::{Mutex, MutexGuard, PoisonError},
    thread,
    time::{Duration, Instant},
};

use quickfix::{dictionary_item::FileStorePath, *};
use utils::*;

mod utils;

const CHILD_TARGET: &str = "QFRS_PANIC_TARGET";
const CHILD_STORE: &str = "QFRS_PANIC_STORE";
const SIGABRT: i32 = 6;

/// The callback that panics in the child process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Target {
    OnCreate,
    OnLogon,
    OnLogout,
    ToAdmin,
    FromAdmin,
    ToApp,
    FromApp,
    LogIncoming,
    LogOutgoing,
    LogEvent,
}

impl Target {
    const ALL: [Self; 10] = [
        Self::OnCreate,
        Self::OnLogon,
        Self::OnLogout,
        Self::ToAdmin,
        Self::FromAdmin,
        Self::ToApp,
        Self::FromApp,
        Self::LogIncoming,
        Self::LogOutgoing,
        Self::LogEvent,
    ];

    fn name(self) -> String {
        format!("{self:?}")
    }

    fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.name() == name)
    }
}

/// Receiver-side application that panics in one callback.
#[derive(Debug)]
struct PanicApp(Option<Target>);

impl PanicApp {
    fn hit(&self, target: Target) {
        if self.0 == Some(target) {
            panic!("injected panic in {target:?}");
        }
    }
}

impl ApplicationCallback for PanicApp {
    fn on_create(&self, _: &SessionId) {
        self.hit(Target::OnCreate);
    }
    fn on_logon(&self, _: &SessionId) {
        self.hit(Target::OnLogon);
    }
    fn on_logout(&self, _: &SessionId) {
        self.hit(Target::OnLogout);
    }
    fn on_msg_to_admin(&self, _: &mut MessageMut<'_>, _: &SessionId) {
        self.hit(Target::ToAdmin);
    }
    fn on_msg_to_app(&self, _: &mut MessageMut<'_>, _: &SessionId) -> Result<(), MsgToAppError> {
        self.hit(Target::ToApp);
        Ok(())
    }
    fn on_msg_from_admin(&self, _: &Message, _: &SessionId) -> Result<(), MsgFromAdminError> {
        self.hit(Target::FromAdmin);
        Ok(())
    }
    fn on_msg_from_app(&self, _: &Message, _: &SessionId) -> Result<(), MsgFromAppError> {
        self.hit(Target::FromApp);
        Ok(())
    }
}

impl LogCallback for PanicApp {
    fn on_incoming(&self, _: Option<&SessionId>, _: &str) {
        self.hit(Target::LogIncoming);
    }
    fn on_outgoing(&self, _: Option<&SessionId>, _: &str) {
        self.hit(Target::LogOutgoing);
    }
    fn on_event(&self, _: Option<&SessionId>, _: &str) {
        self.hit(Target::LogEvent);
    }
}

fn wait_until(timeout: Duration, mut done: impl FnMut() -> bool) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if done() {
            return true;
        }
        thread::sleep(Duration::from_millis(20));
    }
    done()
}

/// Child body: sender (initiator) and receiver (acceptor, FileStore) log on and exchange one
/// News each way; `target` panics on the receiver side.
/// Sender and receiver settings with both FileStores in `store` (QuickFIX prefixes each file with
/// its session), so a later run picks up the sequence numbers and messages where this one ended.
fn file_store_settings(
    port: u16,
    store: &Path,
) -> Result<(SessionSettings, SessionSettings), QuickFixError> {
    let store_path = store.to_str().expect("utf-8 store path");
    let mut sender = build_settings(ServerType::Sender, port)?;
    let mut receiver = build_settings(ServerType::Receiver, port)?;
    for settings in [&mut sender, &mut receiver] {
        settings.set(
            None,
            Dictionary::try_from_items(&[&FileStorePath(store_path)])?,
        )?;
    }
    Ok((sender, receiver))
}

fn run_child(target: Target, store: &Path) -> Result<(), QuickFixError> {
    let (settings_sender, settings_receiver) = file_store_settings(find_available_port(), store)?;

    let sender = NullFixApplication;
    let receiver = PanicApp(Some(target));
    let quiet = PanicApp(None);
    let (receiver_log, sender_log) = (
        LogFactory::try_new(&receiver)?,
        LogFactory::try_new(&quiet)?,
    );
    let app_sender = Application::try_new(&sender)?;
    let app_receiver = Application::try_new(&receiver)?;
    let store_sender = FileMessageStoreFactory::try_new(&settings_sender)?;
    let store_receiver = FileMessageStoreFactory::try_new(&settings_receiver)?;

    let kind = FixSocketServerKind::MultiThreaded;
    let mut acceptor = Acceptor::try_new(
        &settings_receiver,
        &app_receiver,
        &store_receiver,
        &receiver_log,
        kind,
    )?;
    let mut initiator = Initiator::try_new(
        &settings_sender,
        &app_sender,
        &store_sender,
        &sender_log,
        kind,
    )?;
    acceptor.start()?;
    initiator.start()?;
    wait_until(Duration::from_secs(10), || {
        initiator.is_logged_on().unwrap_or(false) && acceptor.is_logged_on().unwrap_or(false)
    });

    send_to_target(
        build_news("to receiver", &[])?,
        &ServerType::Sender.session_id(),
    )?;
    thread::sleep(Duration::from_millis(500));
    send_to_target(
        build_news("to sender", &[])?,
        &ServerType::Receiver.session_id(),
    )?;
    thread::sleep(Duration::from_millis(500));

    initiator.stop()?;
    acceptor.stop()?;
    Ok(())
}

/// Not a test of its own: the entry point of the child process `abort_status` starts.
#[test]
fn child_process_entry() {
    let (Ok(name), Ok(store)) = (env::var(CHILD_TARGET), env::var(CHILD_STORE)) else {
        return;
    };
    let target = Target::from_name(&name).expect("known target");
    run_child(target, Path::new(&store)).expect("child session ran");
}

fn store_dir(label: &str) -> PathBuf {
    let dir = env::temp_dir().join(format!("qfrs-panic-{}-{label}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("store dir");
    dir
}

/// Runs the child for `target`; returns the signal that ended it (None when it exited).
fn child_signal(target: Target, store: &Path) -> Option<i32> {
    let out = Command::new(env::current_exe().expect("test binary path"))
        .args([
            "--exact",
            "child_process_entry",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD_TARGET, target.name())
        .env(CHILD_STORE, store)
        .output()
        .expect("child started");
    out.status.signal()
}

/// The receiver's next expected target seqnum, read from its FileStore.
fn receiver_next_target_seqnum(store: &Path) -> u32 {
    let file = fs::read_dir(store)
        .expect("store dir readable")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .find(|p| p.to_string_lossy().ends_with("RECEIVER-SENDER.seqnums"))
        .expect("receiver seqnums file");
    let text = fs::read_to_string(file).expect("seqnums readable");
    let target = text.split(':').nth(1).expect("sender : target");
    target.trim().parse().expect("seqnum")
}

/// One FIX pair at a time (shared ports, QuickFIX globals); a failed test must not fail the others.
fn serial() -> MutexGuard<'static, u8> {
    checker::GLOBAL_LOCK
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

#[test]
fn every_callback_panic_aborts_the_process() {
    let _lock = serial();
    let survived: Vec<_> = Target::ALL
        .into_iter()
        .filter(|&t| child_signal(t, &store_dir(&t.name())) != Some(SIGABRT))
        .collect();
    assert!(survived.is_empty(), "no abort for a panic in {survived:?}");
}

#[test]
fn from_app_panic_leaves_the_message_unconsumed() {
    let _lock = serial();
    let store = store_dir("unconsumed");
    assert_eq!(child_signal(Target::FromApp, &store), Some(SIGABRT));
    // Logon was 34=1 and the News 34=2: the store must still expect 2, so the peer resends it.
    assert_eq!(receiver_next_target_seqnum(&store), 2);
}

/// Headlines (148) of the News messages the receiver handled.
#[derive(Debug, Default)]
struct Headlines(Mutex<Vec<String>>);

impl Headlines {
    fn contains(&self, headline: &str) -> bool {
        let seen = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        seen.iter().any(|h| h == headline)
    }
}

impl ApplicationCallback for Headlines {
    fn on_msg_from_app(&self, msg: &Message, _: &SessionId) -> Result<(), MsgFromAppError> {
        if let Some(headline) = msg.get_field(MSG_HEADLINE) {
            let mut seen = self.0.lock().unwrap_or_else(PoisonError::into_inner);
            seen.push(headline);
        }
        Ok(())
    }
}

/// The other half of R-P0-1: after the abort, a restarted pair on the same stores delivers the
/// message the aborted process never handled (the receiver asks, the sender resends it).
#[test]
fn from_app_panic_message_is_delivered_after_restart() -> Result<(), QuickFixError> {
    let _lock = serial();
    let store = store_dir("restart");
    assert_eq!(child_signal(Target::FromApp, &store), Some(SIGABRT));

    let (settings_sender, settings_receiver) = file_store_settings(find_available_port(), &store)?;
    let (sender, receiver) = (NullFixApplication, Headlines::default());
    let log = LogFactory::try_new(&NullLogger)?;
    let app_sender = Application::try_new(&sender)?;
    let app_receiver = Application::try_new(&receiver)?;
    let store_sender = FileMessageStoreFactory::try_new(&settings_sender)?;
    let store_receiver = FileMessageStoreFactory::try_new(&settings_receiver)?;

    let kind = FixSocketServerKind::MultiThreaded;
    let mut acceptor = Acceptor::try_new(
        &settings_receiver,
        &app_receiver,
        &store_receiver,
        &log,
        kind,
    )?;
    let mut initiator =
        Initiator::try_new(&settings_sender, &app_sender, &store_sender, &log, kind)?;
    acceptor.start()?;
    initiator.start()?;
    let delivered = wait_until(Duration::from_secs(10), || receiver.contains("to receiver"));
    initiator.stop()?;
    acceptor.stop()?;

    assert!(
        delivered,
        "the aborted message was resent and handled after the restart"
    );
    Ok(())
}

/// How the receiver answers inbound application messages in the reject tests.
#[derive(Debug, Clone, Copy)]
enum Answer {
    FieldNotFound,
    IncorrectDataFormat,
    IncorrectTagValue,
    UnsupportedMessageType,
}

/// A reply seen by the sender: MsgType (35), SessionRejectReason (373), BusinessRejectReason (380).
type Reply = (String, Option<String>, Option<String>);

#[derive(Debug, Default)]
struct Inbox(Mutex<Vec<Reply>>);

impl Inbox {
    fn record(&self, msg: &Message) {
        let msg_type = msg.with_header(|h| h.get_field(35)).unwrap_or_default();
        let reply = (msg_type, msg.get_field(373), msg.get_field(380));
        self.0.lock().expect("inbox poisoned").push(reply);
    }

    fn rejects(&self) -> Vec<Reply> {
        let all = self.0.lock().expect("inbox poisoned");
        all.iter()
            .filter(|r| r.0 == "3" || r.0 == "j")
            .cloned()
            .collect()
    }
}

impl ApplicationCallback for Inbox {
    fn on_msg_from_admin(&self, msg: &Message, _: &SessionId) -> Result<(), MsgFromAdminError> {
        self.record(msg);
        Ok(())
    }
    fn on_msg_from_app(&self, msg: &Message, _: &SessionId) -> Result<(), MsgFromAppError> {
        self.record(msg);
        Ok(())
    }
}

#[derive(Debug)]
struct Rejecting(Answer);

impl ApplicationCallback for Rejecting {
    fn on_msg_from_app(&self, _: &Message, _: &SessionId) -> Result<(), MsgFromAppError> {
        Err(match self.0 {
            Answer::FieldNotFound => MsgFromAppError::FieldNotFound,
            Answer::IncorrectDataFormat => MsgFromAppError::IncorrectDataFormat,
            Answer::IncorrectTagValue => MsgFromAppError::IncorrectTagValue,
            Answer::UnsupportedMessageType => MsgFromAppError::UnsupportedMessageType,
        })
    }
}

/// The rejects the sender gets back for one News when the receiver answers `answer`.
fn rejects_for(answer: Answer) -> Result<Vec<Reply>, QuickFixError> {
    let _lock = serial();
    let port = find_available_port();
    let settings_sender = build_settings(ServerType::Sender, port)?;
    let settings_receiver = build_settings(ServerType::Receiver, port)?;
    let (sender, receiver) = (Inbox::default(), Rejecting(answer));
    let log = LogFactory::try_new(&NullLogger)?;
    let app_sender = Application::try_new(&sender)?;
    let app_receiver = Application::try_new(&receiver)?;
    let (store_sender, store_receiver) = (
        MemoryMessageStoreFactory::new(),
        MemoryMessageStoreFactory::new(),
    );

    let kind = FixSocketServerKind::MultiThreaded;
    let mut acceptor = Acceptor::try_new(
        &settings_receiver,
        &app_receiver,
        &store_receiver,
        &log,
        kind,
    )?;
    let mut initiator =
        Initiator::try_new(&settings_sender, &app_sender, &store_sender, &log, kind)?;
    acceptor.start()?;
    initiator.start()?;
    let logged_on = wait_until(Duration::from_secs(10), || {
        initiator.is_logged_on().unwrap_or(false) && acceptor.is_logged_on().unwrap_or(false)
    });
    assert!(logged_on, "sessions logged on");

    send_to_target(
        build_news("please reject", &[])?,
        &ServerType::Sender.session_id(),
    )?;
    wait_until(Duration::from_secs(3), || !sender.rejects().is_empty());

    initiator.stop()?;
    acceptor.stop()?;
    Ok(sender.rejects())
}

fn reply(msg_type: &str, tag_373: Option<&str>, tag_380: Option<&str>) -> Reply {
    (
        msg_type.to_string(),
        tag_373.map(String::from),
        tag_380.map(String::from),
    )
}

#[test]
fn from_app_unsupported_message_type_sends_business_reject_380_3() -> Result<(), QuickFixError> {
    assert_eq!(
        rejects_for(Answer::UnsupportedMessageType)?,
        [reply("j", None, Some("3"))]
    );
    Ok(())
}

#[test]
fn from_app_field_not_found_sends_business_reject_380_5() -> Result<(), QuickFixError> {
    assert_eq!(
        rejects_for(Answer::FieldNotFound)?,
        [reply("j", None, Some("5"))]
    );
    Ok(())
}

#[test]
fn from_app_incorrect_data_format_sends_reject_373_6() -> Result<(), QuickFixError> {
    assert_eq!(
        rejects_for(Answer::IncorrectDataFormat)?,
        [reply("3", Some("6"), None)]
    );
    Ok(())
}

#[test]
fn from_app_incorrect_tag_value_sends_reject_373_5() -> Result<(), QuickFixError> {
    assert_eq!(
        rejects_for(Answer::IncorrectTagValue)?,
        [reply("3", Some("5"), None)]
    );
    Ok(())
}
