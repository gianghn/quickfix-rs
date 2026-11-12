//! R-P1-2: a binding call that succeeds must not leave the previous call's error behind, or the
//! next failure that sets no error of its own reports the old one.

use std::ffi::CString;

use quickfix_ffi::*;

#[test]
fn successful_call_clears_previous_error() {
    let missing = CString::new("/no/such/settings.ini").expect("no NUL");
    // SAFETY: plain FFI calls with valid pointers; the objects created are deleted below.
    unsafe {
        assert!(FixSessionSettings_fromPath(missing.as_ptr()).is_none());
        assert_ne!(Fix_getLastErrorCode(), 0);
        assert!(Fix_getLastErrorMessage().is_some());

        let settings = FixSessionSettings_new().expect("empty settings");
        assert_eq!(Fix_getLastErrorCode(), 0);
        assert!(Fix_getLastErrorMessage().is_none());
        FixSessionSettings_delete(settings);
    }
}
