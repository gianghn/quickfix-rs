//! Phase 23: the C++ exception a binding call caught maps to the same error code after the catch
//! chain moved out of `CATCH_OR_RETURN` into helper functions. Characterisation: these codes are
//! what the binding returned before the move.

use std::ffi::CString;

use quickfix_ffi::*;

const ERROR_FIELD_NOT_FOUND: i8 = -11;
const ERROR_INVALID_MESSAGE: i8 = -14;
const ERROR_CONFIG_ERROR: i8 = -15;
const ERROR_SESSION_NOT_FOUND: i8 = -33;

fn c(text: &str) -> CString {
    CString::new(text).expect("no NUL")
}

#[test]
fn missing_settings_file_is_a_config_error() {
    let path = c("/no/such/settings.ini");
    // SAFETY: plain FFI call with a valid pointer; it fails and creates nothing.
    unsafe {
        assert!(FixSessionSettings_fromPath(path.as_ptr()).is_none());
        assert_eq!(Fix_getLastErrorCode(), ERROR_CONFIG_ERROR);
    }
}

#[test]
fn missing_header_field_is_field_not_found() {
    // SAFETY: the message is created and deleted here; the header is borrowed from it.
    unsafe {
        let msg = FixMessage_new().expect("new message");
        let header = FixMessage_getHeaderRef(msg).expect("header");
        assert!(FixHeader_getField(header, 129).is_none());
        assert_eq!(Fix_getLastErrorCode(), ERROR_FIELD_NOT_FOUND);
        FixMessage_delete(msg);
    }
}

#[test]
fn unparsable_text_is_an_invalid_message() {
    let text = c("not a FIX message");
    // SAFETY: plain FFI call with a valid pointer; it fails and creates nothing.
    unsafe {
        assert!(FixMessage_fromString(text.as_ptr()).is_none());
        assert_eq!(Fix_getLastErrorCode(), ERROR_INVALID_MESSAGE);
    }
}

#[test]
fn non_numeric_setting_read_as_int_is_a_config_error() {
    let (name, key, value) = (c("SESSION"), c("HeartBtInt"), c("abc"));
    // SAFETY: the dictionary is created and deleted here; `out` is a valid i32.
    unsafe {
        let dict = FixDictionary_new(name.as_ptr()).expect("new dictionary");
        assert_eq!(
            FixDictionary_setString(dict, key.as_ptr(), value.as_ptr()),
            0
        );
        let mut out = 0_i32;
        assert!(FixDictionary_getInt(dict, key.as_ptr(), &mut out) < 0);
        assert_eq!(Fix_getLastErrorCode(), ERROR_CONFIG_ERROR);
        FixDictionary_delete(dict);
    }
}

#[test]
fn send_to_an_unknown_session_is_session_not_found() {
    let (begin, sender, target, qualifier) = (c("FIX.4.4"), c("NOBODY"), c("NOWHERE"), c(""));
    // SAFETY: the message and session id are created and deleted here.
    unsafe {
        let msg = FixMessage_new().expect("new message");
        let id = FixSessionID_new(
            begin.as_ptr(),
            sender.as_ptr(),
            target.as_ptr(),
            qualifier.as_ptr(),
        )
        .expect("new session id");
        assert!(FixSession_sendToTarget(msg, id) < 0);
        assert_eq!(Fix_getLastErrorCode(), ERROR_SESSION_NOT_FOUND);
        FixSessionID_delete(id);
        FixMessage_delete(msg);
    }
}
