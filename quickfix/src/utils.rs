use std::{
    ffi::{self, CStr},
    panic::{catch_unwind, AssertUnwindSafe},
    process,
    ptr::NonNull,
};

use crate::QuickFixError;

/// Runs a user callback called from C++ and aborts the process if it panics (R-P0-1).
///
/// Unwinding into C++ is undefined behaviour, and reporting the panic as success would lose an
/// inbound message (its seqnum consumed) or send an outbound one. After an abort the message is
/// neither: the store still expects it, and the peer resends it after the restart.
pub fn abort_on_panic<T>(callback: impl FnOnce() -> T) -> T {
    catch_unwind(AssertUnwindSafe(callback)).unwrap_or_else(|_| process::abort())
}

#[inline(always)]
pub fn read_checked_cstr(val: NonNull<ffi::c_char>) -> String {
    let cstr = unsafe { CStr::from_ptr(val.as_ptr()) };
    cstr.to_string_lossy().to_string()
}

#[inline(always)]
pub fn ffi_code_to_result(code: i8) -> Result<(), QuickFixError> {
    match code {
        0 => Ok(()),
        _ => Err(QuickFixError::from_last_error()),
    }
}

#[inline(always)]
pub fn ffi_code_to_bool(code: i8) -> Result<bool, QuickFixError> {
    match code {
        1 => Ok(true),
        0 => Ok(false),
        _ => Err(QuickFixError::from_last_error()),
    }
}
