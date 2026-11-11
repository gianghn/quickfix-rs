//! R-P2-1: in a build without SSL, asking for an SSL acceptor or initiator must return an error.
//! The stub used to `throw new std::logic_error(...)`: a pointer no `catch` in the binding
//! matches, so the process terminated.
#![cfg(not(feature = "build-with-ssl"))]

use std::{
    ffi::{self, CString},
    fs,
    ptr::NonNull,
};

use quickfix_ffi::*;

/// `ERRNO_EXCEPTION` in quickfix_bind.h: a `std::exception` the binding has no code for.
const ERRNO_EXCEPTION: i8 = -2;

extern "C" fn on_session(_: *const ffi::c_void, _: FixSessionID_t) {}
extern "C" fn on_msg(_: *const ffi::c_void, _: FixMessage_t, _: FixSessionID_t) {}
extern "C" fn on_msg_code(_: *const ffi::c_void, _: FixMessage_t, _: FixSessionID_t) -> i8 {
    CALLBACK_OK
}
extern "C" fn on_log(
    _: *const ffi::c_void,
    _: Option<FixSessionID_t>,
    _: *const ffi::c_char,
    _: u64,
) {
}

const APP: FixApplicationCallbacks_t = FixApplicationCallbacks_t {
    onCreate: on_session,
    onLogon: on_session,
    onLogout: on_session,
    toAdmin: on_msg,
    toApp: on_msg_code,
    fromAdmin: on_msg_code,
    fromApp: on_msg_code,
};

const LOG: FixLogCallbacks_t = FixLogCallbacks_t {
    onIncoming: on_log,
    onOutgoing: on_log,
    onEvent: on_log,
};

struct Parts {
    app: FixApplication_t,
    store: FixMessageStoreFactory_t,
    settings: FixSessionSettings_t,
    log: FixLogFactory_t,
}

fn parts(connection_type: &str) -> Parts {
    let dir = std::env::temp_dir().join(format!("qfrs-ssl-stub-{connection_type}"));
    fs::create_dir_all(&dir).expect("temp dir");
    let ini = dir.join("settings.ini");
    let body = format!(
        "[DEFAULT]\nConnectionType={connection_type}\nReconnectInterval=60\nSenderCompID=A\n\
         [SESSION]\nBeginString=FIX.4.4\nTargetCompID=B\nStartTime=00:00:00\nEndTime=23:59:59\n\
         HeartBtInt=20\nSocketAcceptPort=1\nSocketConnectHost=127.0.0.1\nSocketConnectPort=1\n\
         UseDataDictionary=N\n"
    );
    fs::write(&ini, body).expect("settings written");
    let path = CString::new(ini.to_str().expect("utf-8 temp path")).expect("no NUL in path");
    let data = NonNull::<u8>::dangling().as_ptr().cast::<ffi::c_void>();

    // SAFETY: every pointer passed lives for the call; the callbacks never read `data`.
    unsafe {
        Parts {
            app: FixApplication_new(data, &APP).expect("application"),
            store: FixMemoryMessageStoreFactory_new().expect("store factory"),
            settings: FixSessionSettings_fromPath(path.as_ptr()).expect("settings"),
            log: FixLogFactory_new(data, &LOG).expect("log factory"),
        }
    }
}

fn delete(p: Parts) {
    // SAFETY: each object was created above and is not used after this.
    unsafe {
        FixLogFactory_delete(p.log);
        FixSessionSettings_delete(p.settings);
        FixMessageStoreFactory_delete(p.store);
        FixApplication_delete(p.app);
    }
}

#[test]
fn fix_acceptor_new_returns_error_when_ssl_is_not_built() {
    for multi_threaded in [0, 1] {
        let p = parts("acceptor");
        // SAFETY: valid objects from `parts`; a null result is checked.
        let acceptor =
            unsafe { FixAcceptor_new(p.app, p.store, p.settings, p.log, multi_threaded, 1) };
        assert!(acceptor.is_none(), "multi_threaded={multi_threaded}");
        // SAFETY: plain reads of the binding's thread-local error state.
        assert_eq!(unsafe { Fix_getLastErrorCode() }, ERRNO_EXCEPTION);
        delete(p);
    }
}

#[test]
fn fix_initiator_new_returns_error_when_ssl_is_not_built() {
    for multi_threaded in [0, 1] {
        let p = parts("initiator");
        // SAFETY: valid objects from `parts`; a null result is checked.
        let initiator =
            unsafe { FixInitiator_new(p.app, p.store, p.settings, p.log, multi_threaded, 1) };
        assert!(initiator.is_none(), "multi_threaded={multi_threaded}");
        // SAFETY: plain reads of the binding's thread-local error state.
        assert_eq!(unsafe { Fix_getLastErrorCode() }, ERRNO_EXCEPTION);
        delete(p);
    }
}
