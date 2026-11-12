use std::{ffi, marker::PhantomData, mem::ManuallyDrop};

use quickfix_ffi::{
    FixApplicationCallbacks_t, FixApplication_delete, FixApplication_new, FixApplication_t,
    FixMessage_t, FixSessionID_t,
};

use crate::{utils::abort_on_panic, Borrowed, Message, MessageMut, QuickFixError, SessionId};

/// Error result that can occurs from a `on_msg_to_app` callback.
#[derive(Debug)]
pub enum MsgToAppError {
    /// Indicates user does not want to send a message
    DoNotSend,
}

/// Error result that can occurs from a `on_msg_from_admin` callback.
#[derive(Debug)]
pub enum MsgFromAdminError {
    /// Field not found inside a message.
    FieldNotFound,
    /// Field has a badly formatted value.
    IncorrectDataFormat,
    /// Field has a value that is out of range.
    IncorrectTagValue,
    /// User wants to reject permission to logon.
    RejectLogon,
}

/// Error result that can occurs from a `on_msg_from_app` callback.
#[derive(Debug)]
pub enum MsgFromAppError {
    /// Field not found inside a message.
    FieldNotFound,
    /// Field has a badly formatted value.
    IncorrectDataFormat,
    /// Field has a value that is out of range.
    IncorrectTagValue,
    /// Message type not supported by application.
    UnsupportedMessageType,
}

trait AsFixCallbackCode {
    fn as_callback_code(&self) -> i8;
}

fn callback_to_code<T: AsFixCallbackCode>(input: Result<(), T>) -> i8 {
    match input {
        Ok(()) => quickfix_ffi::CALLBACK_OK,
        Err(x) => x.as_callback_code(), // Deliberately change the control flow.
    }
}

impl AsFixCallbackCode for MsgToAppError {
    fn as_callback_code(&self) -> i8 {
        quickfix_ffi::CALLBACK_RESULT_DO_NOT_SEND
    }
}

impl AsFixCallbackCode for MsgFromAdminError {
    fn as_callback_code(&self) -> i8 {
        match self {
            Self::FieldNotFound => quickfix_ffi::CALLBACK_RESULT_FIELD_NOT_FOUND,
            Self::IncorrectDataFormat => quickfix_ffi::CALLBACK_RESULT_INCORRECT_DATA_FORMAT,
            Self::IncorrectTagValue => quickfix_ffi::CALLBACK_RESULT_INCORRECT_TAG_VALUE,
            Self::RejectLogon => quickfix_ffi::CALLBACK_RESULT_REJECT_LOGON,
        }
    }
}

impl AsFixCallbackCode for MsgFromAppError {
    fn as_callback_code(&self) -> i8 {
        match self {
            Self::FieldNotFound => quickfix_ffi::CALLBACK_RESULT_FIELD_NOT_FOUND,
            Self::IncorrectDataFormat => quickfix_ffi::CALLBACK_RESULT_INCORRECT_DATA_FORMAT,
            Self::IncorrectTagValue => quickfix_ffi::CALLBACK_RESULT_INCORRECT_TAG_VALUE,
            Self::UnsupportedMessageType => quickfix_ffi::CALLBACK_RESULT_UNSUPPORTED_MESSAGE_TYPE,
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
/// The message given to `on_msg_to_admin` and `on_msg_to_app` belongs to QuickFIX: it can be
/// edited but not moved out, which would free it under QuickFIX (R-P0-3):
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
    fn on_msg_to_app(
        &self,
        _msg: &mut MessageMut<'_>,
        _session: &SessionId,
    ) -> Result<(), MsgToAppError> {
        Ok(())
    }

    /// Called after received a message from admin level.
    fn on_msg_from_admin(
        &self,
        _msg: &Message,
        _session: &SessionId,
    ) -> Result<(), MsgFromAdminError> {
        Ok(())
    }

    /// Called after received a message from application level.
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
