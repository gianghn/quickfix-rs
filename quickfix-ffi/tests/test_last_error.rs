//! R-P1-2: a binding call that succeeds must not leave the previous call's error behind, or the
//! next failure that sets no error of its own reports the old one. Phase 22 keeps these rows green
//! while a successful call stops touching the error text.

use std::ffi::{CStr, CString};
use std::thread;

use quickfix_ffi::*;

/// The current error text, copied out, or `None`.
unsafe fn last_error_text() -> Option<String> {
    Fix_getLastErrorMessage().map(|p| CStr::from_ptr(p.as_ptr()).to_string_lossy().into_owned())
}

#[test]
fn failed_call_after_a_success_reports_its_own_error() {
    let missing = CString::new("/no/such/settings.ini").expect("no NUL");
    // SAFETY: plain FFI calls with valid pointers; the objects created are deleted below.
    unsafe {
        let settings = FixSessionSettings_new().expect("empty settings");
        FixSessionSettings_delete(settings);
        assert!(FixSessionSettings_fromPath(missing.as_ptr()).is_none());
        assert_ne!(Fix_getLastErrorCode(), 0);
        assert!(last_error_text().is_some());
    }
}

#[test]
fn second_failure_replaces_the_first_error() {
    let settings_path = CString::new("/no/such/settings.ini").expect("no NUL");
    let dictionary_path = CString::new("/no/such/dictionary.xml").expect("no NUL");
    // SAFETY: plain FFI calls with valid pointers; both calls fail and create nothing.
    unsafe {
        assert!(FixSessionSettings_fromPath(settings_path.as_ptr()).is_none());
        let first = last_error_text().expect("first error");
        assert!(FixDataDictionary_fromPath(dictionary_path.as_ptr()).is_none());
        let second = last_error_text().expect("second error");
        assert_ne!(first, second);
    }
}

#[test]
fn successful_call_on_a_new_thread_reports_no_error() {
    thread::spawn(|| {
        // SAFETY: plain FFI calls; the settings created are deleted below.
        unsafe {
            let settings = FixSessionSettings_new().expect("empty settings");
            assert_eq!(Fix_getLastErrorCode(), 0);
            assert!(Fix_getLastErrorMessage().is_none());
            FixSessionSettings_delete(settings);
        }
    })
    .join()
    .expect("thread ran");
}

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
