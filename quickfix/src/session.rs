use std::{
    cell::Cell,
    fmt,
    marker::PhantomData,
    sync::{PoisonError, RwLock},
};

use quickfix_ffi::{
    FixSession_isLoggedOn, FixSession_logon, FixSession_logout, FixSession_lookup,
    FixSession_reset, FixSession_send, FixSession_sendToTarget, FixSession_setNextSenderMsgSeqNum,
    FixSession_setNextTargetMsgSeqNum, FixSession_t,
};

use crate::{
    utils::{ffi_code_to_bool, ffi_code_to_result},
    Message, QuickFixError, SessionId,
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
/// Call it after `stop()`: stop joins QuickFIX's threads, whose callbacks may send, so holding the
/// gate across it could deadlock.
pub(crate) fn while_deleting<T>(f: impl FnOnce() -> T) -> T {
    let _gate = SESSION_GATE.write().unwrap_or_else(PoisonError::into_inner);
    f()
}

/// Send message to target design in session ID.
pub fn send_to_target(msg: Message, session_id: &SessionId) -> Result<(), QuickFixError> {
    // NOTE: Message may be changed by real library. Just consume it so nothing will leak to rust code.
    while_sending(|| ffi_code_to_result(unsafe { FixSession_sendToTarget(msg.0, session_id.0) }))
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
            None => Err(QuickFixError::from_last_error()),
        }
    }
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
        while_sending(|| ffi_code_to_bool(unsafe { FixSession_send(self.inner, msg.0) }))
    }

    /// Reset session by sending a logout & disconnecting, but still keeping the session enabled,
    /// so that logon is retried.
    pub fn reset(&mut self) -> Result<(), QuickFixError> {
        ffi_code_to_result(unsafe { FixSession_reset(self.inner) })
    }

    /// Enable session so that logon is sent.
    pub fn logon(&mut self) -> Result<(), QuickFixError> {
        ffi_code_to_result(unsafe { FixSession_logon(self.inner) })
    }
    /// Set next sender message sequence number.
    pub fn set_next_sender_msg_seq_num(&mut self, num: i32) -> Result<(), QuickFixError> {
        ffi_code_to_result(unsafe { FixSession_setNextSenderMsgSeqNum(self.inner, num) })
    }
    /// Set next target message sequence number.
    pub fn set_next_target_msg_seq_num(&mut self, num: i32) -> Result<(), QuickFixError> {
        ffi_code_to_result(unsafe { FixSession_setNextTargetMsgSeqNum(self.inner, num) })
    }
    /// Get expected target message sequence number.
    pub fn get_expected_target_num(&self) -> i32 {
        unsafe { quickfix_ffi::FixSession_getExpectedTargetNum(self.inner) }
    }
    /// Get expected sender message sequence number.
    pub fn get_expected_sender_num(&self) -> i32 {
        unsafe { quickfix_ffi::FixSession_getExpectedSenderNum(self.inner) }
    }
}

impl fmt::Debug for Session<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Session").finish()
    }
}
