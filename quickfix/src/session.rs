use std::{
    cell::Cell,
    fmt,
    marker::PhantomData,
    sync::{PoisonError, RwLock},
};

use quickfix_ffi::{
    FixSession_isEnabled, FixSession_isLoggedOn, FixSession_logon, FixSession_logout,
    FixSession_lookup, FixSession_refresh, FixSession_reset, FixSession_send,
    FixSession_sendToTarget, FixSession_setNextSenderMsgSeqNum, FixSession_setNextTargetMsgSeqNum,
    FixSession_t,
};

use crate::{
    utils::{ffi_code_to_bool, ffi_code_to_result},
    FieldMap, Message, QuickFixError, SessionId,
};

/// Sends hold this gate shared; deleting an acceptor or initiator, which frees its C++ sessions,
/// holds it exclusively. Without it, a send on another thread used the freed session (R-P0-6).
static SESSION_GATE: RwLock<()> = RwLock::new(());

thread_local! {
    // A send made from a callback that runs inside a send already holds the gate on this thread.
    static SEND_DEPTH: Cell<u32> = const { Cell::new(0) };
}

/// Runs a send (`f`) while no acceptor or initiator is being deleted.
pub(crate) fn while_sending<T>(f: impl FnOnce() -> T) -> T {
    let depth = SEND_DEPTH.with(|d| d.replace(d.get().saturating_add(1)));
    let gate = (depth == 0).then(|| SESSION_GATE.read().unwrap_or_else(PoisonError::into_inner));
    let out = f();
    drop(gate);
    SEND_DEPTH.with(|d| d.set(depth));
    out
}

/// Runs a delete (`f`) once no send is in flight, and keeps new sends out until it is done.
///
/// Call it after `stop()`: stop joins `QuickFIX`'s threads, whose callbacks may send, so holding the
/// gate across it could deadlock.
pub(crate) fn while_deleting<T>(f: impl FnOnce() -> T) -> T {
    let _gate = SESSION_GATE.write().unwrap_or_else(PoisonError::into_inner);
    f()
}

/// Send message to target design in session ID.
///
/// `Ok` means `QuickFIX` wrote the message to the counterparty now.
///
/// # Errors
///
/// - `QuickFixError::NotSent` when it did not (R-P0-12): the session is not logged on, the write
///   failed, or `on_msg_to_app` returned `DoNotSend`. A message stored but not written may still
///   go out later as a PossDup resend, so check its age when it is resent (in `on_msg_to_app`).
/// - `QuickFixError::SessionNotFound` when no started acceptor or initiator has `session_id`.
pub fn send_to_target(msg: Message, session_id: &SessionId) -> Result<(), QuickFixError> {
    // NOTE: Message may be changed by real library. Just consume it so nothing will leak to rust code.
    // SAFETY: `msg` and `session_id` own live C++ objects for the call; the gate keeps any session
    // the id finds from being deleted mid-send.
    let written = while_sending(|| {
        ffi_code_to_bool(unsafe { FixSession_sendToTarget(msg.0, session_id.0) })
    })?;
    if written {
        return Ok(());
    }
    Err(QuickFixError::NotSent(format!(
        "{session_id:?}: not logged on, write failed, or refused by on_msg_to_app"
    )))
}

/// FIX Session.
pub struct Session<'a> {
    pub(crate) inner: FixSession_t,
    pub(crate) phantom_container: PhantomData<&'a ()>,
}

impl Session<'static> {
    /// Find a session by its ID.
    ///
    /// # Safety
    ///
    /// Function is unsafe because there is no way to bind FIX session lifetime
    /// to rust session lifetime.
    ///
    /// Use `SessionContainer::session` instead. It will give you a safe scope
    /// where session has been borrowed to the acceptor / initiator.
    pub unsafe fn lookup(session_id: &SessionId) -> Result<Self, QuickFixError> {
        match unsafe { FixSession_lookup(session_id.0) } {
            Some(inner) => Ok(Self {
                inner,
                phantom_container: PhantomData,
            }),
            // Not `from_last_error`: no exception was raised, so the thread's last error belongs
            // to some earlier call (R-P1-2).
            None => Err(QuickFixError::SessionNotFound(format!(
                "No session found: {session_id:?}"
            ))),
        }
    }
}

/// A seqnum as `QuickFIX` takes it: from 1 to `i32::MAX`.
fn seq_num_arg(num: u64) -> Result<i32, QuickFixError> {
    i32::try_from(num).ok().filter(|n| *n >= 1).ok_or_else(|| {
        QuickFixError::invalid_argument(format!("seqnum {num} is not in 1..={}", i32::MAX))
    })
}

/// A seqnum the binding returned: negative values are its error codes, not seqnums.
fn seq_num_result(code: i32) -> Result<u64, QuickFixError> {
    u64::try_from(code).map_err(|_| QuickFixError::from_last_error())
}

impl Session<'_> {
    /// Force session logout, and disable session.
    pub fn logout(&mut self) -> Result<(), QuickFixError> {
        ffi_code_to_result(unsafe { FixSession_logout(self.inner) })
    }

    /// Check if session is logged on.
    pub fn is_logged_on(&mut self) -> Result<bool, QuickFixError> {
        ffi_code_to_bool(unsafe { FixSession_isLoggedOn(self.inner) })
    }

    /// Send message using current session.
    pub fn send(&mut self, msg: Message) -> Result<bool, QuickFixError> {
        // SAFETY: `self.inner` belongs to the acceptor or initiator `self` borrows (or that
        // `lookup`'s caller keeps alive), `msg` owns a live message, and the gate keeps the
        // session from being deleted mid-send.
        while_sending(|| ffi_code_to_bool(unsafe { FixSession_send(self.inner, msg.0) }))
    }

    /// Reset session by sending a logout & disconnecting, but still keeping the session enabled,
    /// so that logon is retried.
    ///
    /// # Warning
    ///
    /// This is a one-sided reset (R-P1-7): it **wipes the message store**, so both sequence
    /// numbers go back to 1 and the stored outbound messages are gone. The counterparty is not
    /// told; unless it resets too, the next logon fails on a sequence gap, and messages it asks
    /// to resend can no longer be resent. To realign with a counterparty, log out and use
    /// [`Session::set_next_sender_msg_seq_num`] / [`Session::set_next_target_msg_seq_num`]
    /// instead (`doc/RUNBOOK_SEQNUM.md`).
    pub fn reset(&mut self) -> Result<(), QuickFixError> {
        ffi_code_to_result(unsafe { FixSession_reset(self.inner) })
    }

    /// Enable session so that logon is sent.
    pub fn logon(&mut self) -> Result<(), QuickFixError> {
        ffi_code_to_result(unsafe { FixSession_logon(self.inner) })
    }

    /// Check if the session is enabled: `logon` enables it, `logout` disables it, and a disabled
    /// session does not log on again by itself.
    ///
    /// # Errors
    ///
    /// The binding's error when `QuickFIX` throws.
    pub fn is_enabled(&mut self) -> Result<bool, QuickFixError> {
        // SAFETY: `self.inner` belongs to the acceptor or initiator `self` borrows (or that
        // `lookup`'s caller keeps alive).
        ffi_code_to_bool(unsafe { FixSession_isEnabled(self.inner) })
    }

    /// Reload the sequence numbers and messages from the message store, for a store that another
    /// process changed.
    ///
    /// # Errors
    ///
    /// The store's error when it cannot be read.
    pub fn refresh(&mut self) -> Result<(), QuickFixError> {
        // SAFETY: `self.inner` belongs to the acceptor or initiator `self` borrows (or that
        // `lookup`'s caller keeps alive).
        ffi_code_to_result(unsafe { FixSession_refresh(self.inner) })
    }

    /// Send a `TestRequest` (35=1) with `test_req_id` (112). The counterparty answers with a
    /// Heartbeat carrying the same `TestReqID`, which proves the link works end to end.
    ///
    /// # Errors
    ///
    /// `test_req_id` holds a NUL byte, or the send fails as in [`Session::send`].
    pub fn send_test_request(&mut self, test_req_id: &str) -> Result<bool, QuickFixError> {
        let mut msg = Message::new();
        msg.with_header_mut(|header| header.set_field(35, "1"))?;
        msg.set_field(112, test_req_id)?;
        self.send(msg)
    }
    /// Set next sender message sequence number.
    ///
    /// `QuickFIX` does not take the session lock here: call it while the session is logged out,
    /// or a concurrent send may use the old number.
    ///
    /// # Errors
    ///
    /// `InvalidArgument` when `num` is 0 or above `i32::MAX`; the store's error otherwise.
    pub fn set_next_sender_msg_seq_num(&mut self, num: u64) -> Result<(), QuickFixError> {
        let num = seq_num_arg(num)?;
        ffi_code_to_result(unsafe { FixSession_setNextSenderMsgSeqNum(self.inner, num) })
    }

    /// Set next target message sequence number. Same rules as
    /// [`Session::set_next_sender_msg_seq_num`].
    ///
    /// # Errors
    ///
    /// `InvalidArgument` when `num` is 0 or above `i32::MAX`; the store's error otherwise.
    pub fn set_next_target_msg_seq_num(&mut self, num: u64) -> Result<(), QuickFixError> {
        let num = seq_num_arg(num)?;
        ffi_code_to_result(unsafe { FixSession_setNextTargetMsgSeqNum(self.inner, num) })
    }

    /// Get expected target message sequence number.
    ///
    /// # Errors
    ///
    /// The binding's error when the store cannot be read (it used to come back as a seqnum).
    pub fn get_expected_target_num(&self) -> Result<u64, QuickFixError> {
        // SAFETY: `self.inner` belongs to the acceptor or initiator `self` borrows (or that
        // `lookup`'s caller keeps alive).
        seq_num_result(unsafe { quickfix_ffi::FixSession_getExpectedTargetNum(self.inner) })
    }

    /// Get expected sender message sequence number.
    ///
    /// # Errors
    ///
    /// The binding's error when the store cannot be read (it used to come back as a seqnum).
    pub fn get_expected_sender_num(&self) -> Result<u64, QuickFixError> {
        // SAFETY: `self.inner` belongs to the acceptor or initiator `self` borrows (or that
        // `lookup`'s caller keeps alive).
        seq_num_result(unsafe { quickfix_ffi::FixSession_getExpectedSenderNum(self.inner) })
    }
}

impl fmt::Debug for Session<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Session").finish()
    }
}
