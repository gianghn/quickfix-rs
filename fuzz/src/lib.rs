//! What the fuzz targets share: the dictionaries they parse with and the parse step itself.
//!
//! The libFuzzer input is raw bytes, parsed as they are: the old research harness decoded them
//! lossily, which changed lengths and so the meaning of `9=`, `95=` and `10=`.

use quickfix::{DataDictionary, Message, QuickFixError};

/// A dictionary the targets parse with, all taken from this repository.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Spec {
    /// Upstream `FIX44.xml`.
    Fix44,
    /// The KRX dictionary of `quickfix-msgkrx`.
    Fix44Krx,
    /// FIXT1.1 session and FIX50 application messages in one file (`quickfix-msg50`).
    Fixt11Fix50,
}

impl Spec {
    /// The dictionary file.
    #[must_use]
    pub const fn path(self) -> &'static str {
        match self {
            Self::Fix44 => concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../quickfix-ffi/libquickfix/spec/FIX44.xml"
            ),
            Self::Fix44Krx => concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../quickfix-msgkrx/src/FIX44KRX.xml"
            ),
            Self::Fixt11Fix50 => concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../quickfix-msg50/src/spec.xml"
            ),
        }
    }

    /// Loads the dictionary.
    ///
    /// # Errors
    ///
    /// The file is missing or `QuickFIX` refuses it.
    pub fn load(self) -> Result<DataDictionary, QuickFixError> {
        DataDictionary::try_from_path(self.path())
    }
}

/// Parses `data` into a message, with `dictionary` when given, and serializes what parsed.
pub fn parse(dictionary: Option<&DataDictionary>, data: &[u8]) {
    let parsed = match dictionary {
        Some(dictionary) => dictionary.try_build_message_bytes(data),
        None => Message::try_from_bytes(data),
    };
    if let Ok(message) = parsed {
        // Serializing walks every parsed field, groups, header and trailer included.
        let _bytes = message.to_fix_bytes();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The R-P0-13 frames that crashed `QuickFIX` before the C++ fork's P0-1 fix (phase 16).
    const FORMER_CRASHES: [&[u8]; 4] = [
        include_bytes!("../seeds/r-p0-13-95-100000"),
        include_bytes!("../seeds/r-p0-13-95-100000000"),
        include_bytes!("../seeds/r-p0-13-95-2000000000"),
        include_bytes!("../seeds/r-p0-13-fuzzer-first-find"),
    ];

    #[test]
    fn former_r_p0_13_crashes_are_refused_by_the_dictionary() -> Result<(), QuickFixError> {
        let dictionary = Spec::Fix44.load()?;
        for crash in FORMER_CRASHES {
            assert!(dictionary.try_build_message_bytes(crash).is_err());
            // No dictionary, no data fields: it may parse, it must not crash.
            parse(None, crash);
        }
        Ok(())
    }

    #[test]
    fn spec_load_returns_dictionary_for_every_spec() -> Result<(), QuickFixError> {
        for spec in [Spec::Fix44, Spec::Fix44Krx, Spec::Fixt11Fix50] {
            spec.load()?;
        }
        Ok(())
    }
}
