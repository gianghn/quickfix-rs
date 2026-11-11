//! Sequence-number APIs and error reporting of the session wrappers (go-live phase 9).
//!
//! - R-P1-1: the getters returned the binding's error codes as if they were seqnums, and the
//!   setters took any `i32`, 0 and negatives included.
//! - R-P1-2: a lookup that found no session reported whatever error the thread saw last.

use std::sync::{MutexGuard, PoisonError};

use quickfix::*;
use utils::*;

mod utils;

fn serial() -> MutexGuard<'static, u8> {
    checker::GLOBAL_LOCK
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

/// Runs `check` on the receiver's session of an acceptor that is created but not started.
fn with_receiver_session(check: impl FnOnce(&mut Session<'_>)) -> Result<(), QuickFixError> {
    let _lock = serial();
    let settings = build_settings(ServerType::Receiver, find_available_port())?;
    let app = NullFixApplication;
    let log = LogFactory::try_new(&NullLogger)?;
    let application = Application::try_new(&app)?;
    let store = MemoryMessageStoreFactory::new();
    let acceptor = Acceptor::try_new(
        &settings,
        &application,
        &store,
        &log,
        FixSocketServerKind::MultiThreaded,
    )?;
    let mut session = acceptor.session(ServerType::Receiver.session_id())?;
    check(&mut session);
    Ok(())
}

#[test]
fn set_next_msg_seq_num_returns_error_when_num_is_zero() -> Result<(), QuickFixError> {
    with_receiver_session(|session| {
        assert!(session.set_next_sender_msg_seq_num(0).is_err());
        assert!(session.set_next_target_msg_seq_num(0).is_err());
        assert_eq!(session.get_expected_sender_num(), Ok(1));
    })
}

#[test]
fn set_next_msg_seq_num_returns_error_when_num_exceeds_i32() -> Result<(), QuickFixError> {
    with_receiver_session(|session| {
        let too_big = u64::try_from(i32::MAX).unwrap() + 1;
        assert!(session.set_next_sender_msg_seq_num(too_big).is_err());
        assert_eq!(session.get_expected_sender_num(), Ok(1));
    })
}

#[test]
fn get_expected_num_returns_what_was_set() -> Result<(), QuickFixError> {
    with_receiver_session(|session| {
        session.set_next_sender_msg_seq_num(100).unwrap();
        session.set_next_target_msg_seq_num(200).unwrap();
        assert_eq!(session.get_expected_sender_num(), Ok(100));
        assert_eq!(session.get_expected_target_num(), Ok(200));
    })
}

#[test]
fn lookup_returns_session_not_found_after_an_unrelated_error() {
    let _lock = serial();
    // Leaves a FieldNotFound in this thread's last-error slot.
    assert_eq!(Message::new().get_field(58), None);

    let unknown = SessionId::try_new("FIX.4.4", "NOBODY", "NOWHERE", "").unwrap();
    // SAFETY: the session is not used; only the error of a failed lookup is.
    let err = unsafe { Session::lookup(&unknown) }.err();
    assert!(
        matches!(err, Some(QuickFixError::SessionNotFound(_))),
        "{err:?}"
    );
}
