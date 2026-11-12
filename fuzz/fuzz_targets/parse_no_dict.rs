//! Raw bytes into a `Message` without a dictionary.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| quickfix_fuzz::parse(None, data));
