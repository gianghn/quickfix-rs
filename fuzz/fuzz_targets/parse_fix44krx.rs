//! Raw bytes into a `Message` with the KRX FIX44 dictionary.
#![no_main]

use libfuzzer_sys::fuzz_target;
use quickfix::{DataDictionary, QuickFixError};
use quickfix_fuzz::Spec;

thread_local! {
    static DICTIONARY: Result<DataDictionary, QuickFixError> = Spec::Fix44Krx.load();
}

// A dictionary that does not load fails the crate's `spec_load_*` test, which CI runs first.
fuzz_target!(|data: &[u8]| DICTIONARY.with(|dictionary| {
    if let Ok(dictionary) = dictionary {
        quickfix_fuzz::parse(Some(dictionary), data);
    }
}));
