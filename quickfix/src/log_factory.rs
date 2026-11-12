use std::{
    ffi, fmt,
    io::{self, Write},
    marker::PhantomData,
    mem::ManuallyDrop,
};

use quickfix_ffi::{
    FixLogCallbacks_t, FixLogFactory_delete, FixLogFactory_new, FixLogFactory_t, FixSessionID_t,
};

use crate::{utils::abort_on_panic, QuickFixError, SessionId};

/// Log event that can occurs in quickfix library.
///
/// Each callback will be called based on session / socket lifecycle, from any `QuickFIX` thread,
/// so implementations must be `Send + Sync` (R-P0-4):
///
/// ```compile_fail
/// use std::cell::Cell;
/// struct LineCounter(Cell<u64>);
/// impl quickfix::LogCallback for LineCounter {}
/// ```
///
/// A panic in any of these methods aborts the process: a log that silently loses lines is an
/// audit-trail hole.
pub trait LogCallback: Send + Sync {
    /// New incoming messages is available.
    fn on_incoming(&self, _session_id: Option<&SessionId>, _msg: &str) {}

    /// New outgoing message will be sent.
    fn on_outgoing(&self, _session_id: Option<&SessionId>, _msg: &str) {}

    /// Other FIX event has occurred.
    fn on_event(&self, _session_id: Option<&SessionId>, _msg: &str) {}

    /// New incoming message, exactly as on the wire (R-P1-3): NULs and non-UTF-8 bytes included.
    ///
    /// The default passes it to [`LogCallback::on_incoming`], with invalid UTF-8 replaced by
    /// U+FFFD. Override it for an exact audit trail.
    fn on_incoming_bytes(&self, session_id: Option<&SessionId>, msg: &[u8]) {
        self.on_incoming(session_id, &String::from_utf8_lossy(msg));
    }

    /// New outgoing message, exactly as on the wire. See [`LogCallback::on_incoming_bytes`].
    fn on_outgoing_bytes(&self, session_id: Option<&SessionId>, msg: &[u8]) {
        self.on_outgoing(session_id, &String::from_utf8_lossy(msg));
    }

    /// Other FIX event, as bytes. See [`LogCallback::on_incoming_bytes`].
    fn on_event_bytes(&self, session_id: Option<&SessionId>, msg: &[u8]) {
        self.on_event(session_id, &String::from_utf8_lossy(msg));
    }
}

/// The `len` bytes `QuickFIX` passed; empty for a null pointer.
///
/// # Safety
///
/// `ptr` must point to `len` readable bytes that live for the call, or be null.
unsafe fn log_bytes<'a>(ptr: *const ffi::c_char, len: u64) -> &'a [u8] {
    match usize::try_from(len) {
        // SAFETY: the caller promises `len` readable bytes at the non-null `ptr` for the call.
        Ok(len) if !ptr.is_null() => unsafe { std::slice::from_raw_parts(ptr.cast(), len) },
        _ => &[],
    }
}

/// Logging factory.
pub struct LogFactory<'a, C: LogCallback>(pub(crate) FixLogFactory_t, PhantomData<&'a C>);

impl<'a, C> LogFactory<'a, C>
where
    C: LogCallback + 'static,
{
    /// Create new struct from given logger trait.
    pub fn try_new(callbacks: &'a C) -> Result<Self, QuickFixError> {
        match unsafe {
            FixLogFactory_new(
                callbacks as *const C as *const ffi::c_void,
                &Self::CALLBACKS,
            )
        } {
            Some(fix_log_factory) => Ok(Self(fix_log_factory, PhantomData)),
            None => Err(QuickFixError::from_last_error()),
        }
    }

    const CALLBACKS: FixLogCallbacks_t = FixLogCallbacks_t {
        onIncoming: Self::on_incoming,
        onOutgoing: Self::on_outgoing,
        onEvent: Self::on_event,
    };

    extern "C" fn on_incoming(
        data: *const ffi::c_void,
        session_id_ptr: Option<FixSessionID_t>,
        msg_ptr: *const ffi::c_char,
        msg_len: u64,
    ) {
        let session_id = session_id_ptr.map(|ptr| ManuallyDrop::new(SessionId(ptr)));
        // SAFETY: the binding passes `msg.data()` and `msg.size()` of a string that outlives the call.
        let msg = unsafe { log_bytes(msg_ptr, msg_len) };

        abort_on_panic(|| {
            let this = unsafe { &*(data as *const C) };
            this.on_incoming_bytes(session_id.as_deref(), msg);
        });
    }

    extern "C" fn on_outgoing(
        data: *const ffi::c_void,
        session_id_ptr: Option<FixSessionID_t>,
        msg_ptr: *const ffi::c_char,
        msg_len: u64,
    ) {
        let session_id = session_id_ptr.map(|ptr| ManuallyDrop::new(SessionId(ptr)));
        // SAFETY: the binding passes `msg.data()` and `msg.size()` of a string that outlives the call.
        let msg = unsafe { log_bytes(msg_ptr, msg_len) };

        abort_on_panic(|| {
            let this = unsafe { &*(data as *const C) };
            this.on_outgoing_bytes(session_id.as_deref(), msg);
        });
    }

    extern "C" fn on_event(
        data: *const ffi::c_void,
        session_id_ptr: Option<FixSessionID_t>,
        msg_ptr: *const ffi::c_char,
        msg_len: u64,
    ) {
        let session_id = session_id_ptr.map(|ptr| ManuallyDrop::new(SessionId(ptr)));
        // SAFETY: the binding passes `msg.data()` and `msg.size()` of a string that outlives the call.
        let msg = unsafe { log_bytes(msg_ptr, msg_len) };

        abort_on_panic(|| {
            let this = unsafe { &*(data as *const C) };
            this.on_event_bytes(session_id.as_deref(), msg);
        });
    }
}

impl<C: LogCallback> fmt::Debug for LogFactory<'_, C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("LogFactory").finish()
    }
}

impl<C: LogCallback> Drop for LogFactory<'_, C> {
    fn drop(&mut self) {
        unsafe { FixLogFactory_delete(self.0) }
    }
}

/// Drop every log message.
#[derive(Debug)]
pub struct NullLogger;

impl LogCallback for NullLogger {}

/// Log message to std file descriptors.
pub enum StdLogger {
    /// Log to stdout.
    Stdout,
    /// Log to stderr.
    Stderr,
}

impl fmt::Debug for StdLogger {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Stdout => write!(f, "log_stdout"),
            Self::Stderr => write!(f, "log_stderr"),
        }
    }
}

impl StdLogger {
    fn print(&self, text: &str) {
        let text = text.replace('\x01', "|");
        let _ = match self {
            StdLogger::Stdout => writeln!(io::stdout(), "{text}"),
            StdLogger::Stderr => writeln!(io::stderr(), "{text}"),
        };
    }
}

impl LogCallback for StdLogger {
    fn on_incoming(&self, session_id: Option<&SessionId>, msg: &str) {
        self.print(&format!("FIX incoming: {session_id:?}: {msg}"));
    }

    fn on_outgoing(&self, session_id: Option<&SessionId>, msg: &str) {
        self.print(&format!("FIX outgoing: {session_id:?}: {msg}"));
    }

    fn on_event(&self, session_id: Option<&SessionId>, msg: &str) {
        self.print(&format!("FIX event: {session_id:?}: {msg}"));
    }
}

/// Log message using `log` crate.
#[derive(Debug)]
#[cfg(feature = "log")]
pub struct RustLogger;

#[cfg(feature = "log")]
impl LogCallback for RustLogger {
    fn on_incoming(&self, session_id: Option<&SessionId>, msg: &str) {
        let msg = msg.replace('\x01', "|");
        log::info!("FIX: Incoming: {session_id:?}: {msg}");
    }

    fn on_outgoing(&self, session_id: Option<&SessionId>, msg: &str) {
        let msg = msg.replace('\x01', "|");
        log::info!("FIX: Outcoming: {session_id:?}: {msg}");
    }

    fn on_event(&self, session_id: Option<&SessionId>, msg: &str) {
        let msg = msg.replace('\x01', "|");
        log::info!("FIX: Event: {session_id:?}: {msg}");
    }
}
