use std::{ffi, marker::PhantomData, mem::ManuallyDrop};

use quickfix_ffi::{
    FixApplicationCallbacks_t, FixApplication_delete, FixApplication_new,
    FixApplication_setRejectDetail, FixApplication_t, FixMessage_t, FixSessionID_t,
};

use crate::{utils::abort_on_panic, Borrowed, Message, MessageMut, QuickFixError, SessionId};

/// Error result that can occurs from a `on_msg_to_app` callback.
#[derive(Debug)]
pub enum MsgToAppError {
    /// Indicates user does not want to send a message
    DoNotSend,
}

/// Error result that can occurs from a `on_msg_from_admin` callback.
///
/// The counterparty sees `tag` as `RefTagID` (371) on the Reject, and `text` as the Text (58) of
/// the Logout that refuses the logon (R-P2-2).
#[derive(Debug)]
pub enum MsgFromAdminError {
    /// Field not found inside a message.
    FieldNotFound {
        /// The missing tag.
        tag: i32,
    },
    /// Field has a badly formatted value.
    IncorrectDataFormat {
        /// The badly formatted tag.
        tag: i32,
    },
    /// Field has a value that is out of range.
    IncorrectTagValue {
        /// The tag with the out-of-range value.
        tag: i32,
    },
    /// User wants to reject permission to logon.
    RejectLogon {
        /// Why: the Logout's Text is `Rejected Logon Attempt: <text>`. Text after a NUL byte is
        /// dropped.
        text: String,
    },
}

/// Error result that can occurs from a `on_msg_from_app` callback.
///
/// A session Reject (35=3) carries `tag` as `RefTagID` (371); a `BusinessMessageReject` (35=j, sent
/// for `FieldNotFound` from FIX 4.2) names it in its Text (R-P2-2).
#[derive(Debug)]
pub enum MsgFromAppError {
    /// Field not found inside a message.
    FieldNotFound {
        /// The missing tag.
        tag: i32,
    },
    /// Field has a badly formatted value.
    IncorrectDataFormat {
        /// The badly formatted tag.
        tag: i32,
    },
    /// Field has a value that is out of range.
    IncorrectTagValue {
        /// The tag with the out-of-range value.
        tag: i32,
    },
    /// Message type not supported by application.
    UnsupportedMessageType,
}

trait AsFixCallbackCode {
    fn as_callback_code(&self) -> i8;

    /// The tag and text `QuickFIX` puts on the reject it sends for this error.
    fn reject_detail(&self) -> (i32, &str) {
        (0, "")
    }
}

fn callback_to_code<T: AsFixCallbackCode>(input: Result<(), T>) -> i8 {
    match input {
        Ok(()) => quickfix_ffi::CALLBACK_OK,
        Err(x) => {
            set_reject_detail(x.reject_detail());
            x.as_callback_code() // Deliberately change the control flow.
        }
    }
}

/// Hands the reject detail to the binding, on the callback's thread, before the code returns.
fn set_reject_detail((tag, text): (i32, &str)) {
    // A C string ends at a NUL: keep the text before it rather than lose all of it.
    let before_nul = text.split('\0').next().unwrap_or_default();
    let text = ffi::CString::new(before_nul).unwrap_or_default();
    // SAFETY: `text` is a NUL-terminated C string that outlives the call; the binding copies it
    // into its thread-local reject detail before returning.
    unsafe { FixApplication_setRejectDetail(tag, text.as_ptr()) }
}

impl AsFixCallbackCode for MsgToAppError {
    fn as_callback_code(&self) -> i8 {
        quickfix_ffi::CALLBACK_RESULT_DO_NOT_SEND
    }
}

impl AsFixCallbackCode for MsgFromAdminError {
    fn as_callback_code(&self) -> i8 {
        match self {
            Self::FieldNotFound { .. } => quickfix_ffi::CALLBACK_RESULT_FIELD_NOT_FOUND,
            Self::IncorrectDataFormat { .. } => quickfix_ffi::CALLBACK_RESULT_INCORRECT_DATA_FORMAT,
            Self::IncorrectTagValue { .. } => quickfix_ffi::CALLBACK_RESULT_INCORRECT_TAG_VALUE,
            Self::RejectLogon { .. } => quickfix_ffi::CALLBACK_RESULT_REJECT_LOGON,
        }
    }

    fn reject_detail(&self) -> (i32, &str) {
        match self {
            Self::FieldNotFound { tag }
            | Self::IncorrectDataFormat { tag }
            | Self::IncorrectTagValue { tag } => (*tag, ""),
            Self::RejectLogon { text } => (0, text),
        }
    }
}

impl AsFixCallbackCode for MsgFromAppError {
    fn as_callback_code(&self) -> i8 {
        match self {
            Self::FieldNotFound { .. } => quickfix_ffi::CALLBACK_RESULT_FIELD_NOT_FOUND,
            Self::IncorrectDataFormat { .. } => quickfix_ffi::CALLBACK_RESULT_INCORRECT_DATA_FORMAT,
            Self::IncorrectTagValue { .. } => quickfix_ffi::CALLBACK_RESULT_INCORRECT_TAG_VALUE,
            Self::UnsupportedMessageType => quickfix_ffi::CALLBACK_RESULT_UNSUPPORTED_MESSAGE_TYPE,
        }
    }

    fn reject_detail(&self) -> (i32, &str) {
        match self {
            Self::FieldNotFound { tag }
            | Self::IncorrectDataFormat { tag }
            | Self::IncorrectTagValue { tag } => (*tag, ""),
            Self::UnsupportedMessageType => (0, ""),
        }
    }
}

/// These methods notify your application about events that happen on active FIX sessions.
///
/// There is no guarantee how many threads will be calling these functions, so implementations
/// must be `Send + Sync` (R-P0-4); state that is not, such as a `Cell`, is rejected:
///
/// ```compile_fail
/// use std::cell::Cell;
/// struct Counter(Cell<u32>);
/// impl quickfix::ApplicationCallback for Counter {}
/// ```
///
/// The message given to `on_msg_to_admin` and `on_msg_to_app` belongs to `QuickFIX`: it can be
/// edited but not moved out, which would free it under `QuickFIX` (R-P0-3):
///
/// ```compile_fail
/// use quickfix::*;
/// struct Thief;
/// impl ApplicationCallback for Thief {
///     fn on_msg_to_app(&self, msg: &mut Message, _: &SessionId) -> Result<(), MsgToAppError> {
///         let _stolen = std::mem::take(msg);
///         Ok(())
///     }
/// }
/// ```
///
/// A panic in any of these methods aborts the process, so the message it was handling is neither
/// consumed nor sent: the store still expects it and the peer resends it after a restart.
pub trait ApplicationCallback: Send + Sync {
    /// On session created.
    fn on_create(&self, _session: &SessionId) {}

    /// On session logon.
    fn on_logon(&self, _session: &SessionId) {}

    /// On session logout.
    fn on_logout(&self, _session: &SessionId) {}

    /// Called before sending message to admin level.
    ///
    /// Message can be updated at this stage.
    fn on_msg_to_admin(&self, _msg: &mut MessageMut<'_>, _session: &SessionId) {}

    /// Called before sending message to application level.
    ///
    /// Message can be updated at this stage.
    ///
    /// # Errors
    ///
    /// [`MsgToAppError::DoNotSend`] stops `QuickFIX` from sending the message.
    fn on_msg_to_app(
        &self,
        _msg: &mut MessageMut<'_>,
        _session: &SessionId,
    ) -> Result<(), MsgToAppError> {
        Ok(())
    }

    /// Called after received a message from admin level.
    ///
    /// # Errors
    ///
    /// A [`MsgFromAdminError`] makes `QuickFIX` reject the message, or refuse the logon.
    fn on_msg_from_admin(
        &self,
        _msg: &Message,
        _session: &SessionId,
    ) -> Result<(), MsgFromAdminError> {
        Ok(())
    }

    /// Called after received a message from application level.
    ///
    /// # Errors
    ///
    /// A [`MsgFromAppError`] makes `QuickFIX` reject the message.
    fn on_msg_from_app(&self, _msg: &Message, _session: &SessionId) -> Result<(), MsgFromAppError> {
        Ok(())
    }
}

/// Application callback wrapper.
#[derive(Debug)]
pub struct Application<'a, C: ApplicationCallback>(pub(crate) FixApplication_t, PhantomData<&'a C>);

impl<'a, C> Application<'a, C>
where
    C: ApplicationCallback + 'static,
{
    /// Try create new struct from its underlying components.
    pub fn try_new(callbacks: &'a C) -> Result<Self, QuickFixError> {
        match unsafe {
            FixApplication_new(
                callbacks as *const C as *const ffi::c_void,
                &Self::CALLBACKS,
            )
        } {
            Some(fix_application) => Ok(Self(fix_application, PhantomData)),
            None => Err(QuickFixError::from_last_error()),
        }
    }

    const CALLBACKS: FixApplicationCallbacks_t = FixApplicationCallbacks_t {
        onCreate: Self::on_create,
        onLogon: Self::on_logon,
        onLogout: Self::on_logout,
        toAdmin: Self::to_admin,
        toApp: Self::to_app,
        fromAdmin: Self::from_admin,
        fromApp: Self::from_app,
    };

    extern "C" fn on_create(data: *const ffi::c_void, session: FixSessionID_t) {
        let session_id = ManuallyDrop::new(SessionId(session));

        abort_on_panic(|| {
            let this = unsafe { &*(data as *const C) };
            this.on_create(&session_id);
        });
    }

    extern "C" fn on_logon(data: *const ffi::c_void, session: FixSessionID_t) {
        let session_id = ManuallyDrop::new(SessionId(session));

        abort_on_panic(|| {
            let this = unsafe { &*(data as *const C) };
            this.on_logon(&session_id);
        });
    }

    extern "C" fn on_logout(data: *const ffi::c_void, session: FixSessionID_t) {
        let session_id = ManuallyDrop::new(SessionId(session));

        abort_on_panic(|| {
            let this = unsafe { &*(data as *const C) };
            this.on_logout(&session_id);
        });
    }

    extern "C" fn to_admin(data: *const ffi::c_void, msg: FixMessage_t, session: FixSessionID_t) {
        let session_id = ManuallyDrop::new(SessionId(session));

        abort_on_panic(|| {
            let this = unsafe { &*(data as *const C) };
            this.on_msg_to_admin(&mut Borrowed::new(Message(msg)), &session_id);
        });
    }

    extern "C" fn to_app(
        data: *const ffi::c_void,
        msg: FixMessage_t,
        session: FixSessionID_t,
    ) -> i8 {
        let session_id = ManuallyDrop::new(SessionId(session));

        let output_code = abort_on_panic(|| {
            let this = unsafe { &*(data as *const C) };
            this.on_msg_to_app(&mut Borrowed::new(Message(msg)), &session_id)
        });

        callback_to_code(output_code)
    }

    extern "C" fn from_admin(
        data: *const ffi::c_void,
        msg: FixMessage_t,
        session: FixSessionID_t,
    ) -> i8 {
        let msg = ManuallyDrop::new(Message(msg));
        let session_id = ManuallyDrop::new(SessionId(session));

        let output_code = abort_on_panic(|| {
            let this = unsafe { &*(data as *const C) };
            this.on_msg_from_admin(&msg, &session_id)
        });

        callback_to_code(output_code)
    }

    extern "C" fn from_app(
        data: *const ffi::c_void,
        msg: FixMessage_t,
        session: FixSessionID_t,
    ) -> i8 {
        let msg = ManuallyDrop::new(Message(msg));
        let session_id = ManuallyDrop::new(SessionId(session));

        let output_code = abort_on_panic(|| {
            let this = unsafe { &*(data as *const C) };
            this.on_msg_from_app(&msg, &session_id)
        });

        callback_to_code(output_code)
    }
}

impl<C: ApplicationCallback> Drop for Application<'_, C> {
    fn drop(&mut self) {
        unsafe { FixApplication_delete(self.0) };
    }
}
