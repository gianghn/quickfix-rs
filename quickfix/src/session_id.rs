use std::{ffi::CString, fmt};

use quickfix_ffi::{
    FixSessionID_copy, FixSessionID_delete, FixSessionID_getBeginString,
    FixSessionID_getSenderCompID, FixSessionID_getSessionQualifier, FixSessionID_getTargetCompID,
    FixSessionID_isFIXT, FixSessionID_new, FixSessionID_t, FixSessionID_toString,
};

use crate::{utils::read_checked_cstr, QuickFixError};

/// Unique session id consists of BeginString, SenderCompID and TargetCompID.
///
/// `Send + Sync`: an app can keep one (a `clone()` of the `&SessionId` a callback gets) and send
/// through it from any thread, or share it in an `Arc`.
pub struct SessionId(pub(crate) FixSessionID_t);

// SAFETY: `FIX::SessionID` changes only in `fromString`, which the binding does not expose. Every
// bound call (`new`, `copy`, `delete`, the getters, `toString`) leaves it as built, and `toString`
// returns the string frozen in its constructor, so threads only ever read it, and the owned copy
// can be freed on any thread. A callback's `&SessionId` is QuickFIX's own and lives only for the
// call.
unsafe impl Send for SessionId {}
// SAFETY: as for `Send`: no bound call writes to the object after construction.
unsafe impl Sync for SessionId {}

impl SessionId {
    /// Try create new struct from all its inner components.
    ///
    /// # Panic
    ///
    /// When memory allocation fail in C++ library.
    pub fn try_new(
        begin_string: &str,
        sender_comp_id: &str,
        target_comp_id: &str,
        session_qualifier: &str,
    ) -> Result<Self, QuickFixError> {
        let ffi_begin_string = CString::new(begin_string)?;
        let ffi_sender_comp_id = CString::new(sender_comp_id)?;
        let ffi_target_comp_id = CString::new(target_comp_id)?;
        let ffi_session_qualifier = CString::new(session_qualifier)?;

        Ok(unsafe {
            // Session ID cannot fail
            FixSessionID_new(
                ffi_begin_string.as_ptr(),
                ffi_sender_comp_id.as_ptr(),
                ffi_target_comp_id.as_ptr(),
                ffi_session_qualifier.as_ptr(),
            )
        }
        .map(Self)
        .expect("Fail to allocate SessionId"))
    }

    /// Get beginning string.
    pub fn get_begin_string(&self) -> Option<String> {
        unsafe { FixSessionID_getBeginString(self.0) }.map(read_checked_cstr)
    }

    /// Get sender comp ID.
    pub fn get_sender_comp_id(&self) -> Option<String> {
        unsafe { FixSessionID_getSenderCompID(self.0) }.map(read_checked_cstr)
    }

    /// Get target comp ID.
    pub fn get_target_comp_id(&self) -> Option<String> {
        unsafe { FixSessionID_getTargetCompID(self.0) }.map(read_checked_cstr)
    }

    /// Get optional session qualifier.
    pub fn get_session_qualifier(&self) -> Option<String> {
        unsafe { FixSessionID_getSessionQualifier(self.0) }.map(read_checked_cstr)
    }

    /// Check if sessions is FIXT or not.
    pub fn is_fixt(&self) -> bool {
        let val = unsafe { FixSessionID_isFIXT(self.0) };
        val != 0
    }

    /// Convert session ID to a nicely printable string.
    pub fn to_repr(&self) -> String {
        unsafe { FixSessionID_toString(self.0) }
            .map(read_checked_cstr)
            .unwrap_or_default()
    }
}

impl Clone for SessionId {
    fn clone(&self) -> Self {
        Self(unsafe { FixSessionID_copy(self.0) }.expect("Fail to copy SessionID"))
    }
}

impl fmt::Debug for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("SessionId").field(&self.to_repr()).finish()
    }
}

impl Drop for SessionId {
    fn drop(&mut self) {
        unsafe { FixSessionID_delete(self.0) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clone_have_distinct_ptr() {
        let session1 = SessionId::try_new("FIX.4.1", "FOO", "BAR", "").unwrap();

        let session2 = session1.clone();
        assert_ne!(session1.0, session2.0);
        assert_eq!(session1.to_repr(), session2.to_repr());
    }
}
