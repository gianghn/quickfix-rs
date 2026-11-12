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
    if cfg!(feature = "skip-r-p0-13") && overreads_data_field(data) {
        return;
    }
    let parsed = match dictionary {
        Some(dictionary) => dictionary.try_build_message_bytes(data),
        None => Message::try_from_bytes(data),
    };
    if let Ok(message) = parsed {
        // Serializing walks every parsed field, groups, header and trailer included.
        let _bytes = message.to_fix_bytes();
    }
}

/// Every data field of the targets' dictionaries, as (length tag, data tag). `QuickFIX` takes a
/// data field's length from the tag before it, and from 93 for Signature (89).
const DATA_FIELDS: [(&[u8], &[u8]); 18] = [
    (b"93=", b"89="),
    (b"90=", b"91="),
    (b"95=", b"96="),
    (b"212=", b"213="),
    (b"348=", b"349="),
    (b"350=", b"351="),
    (b"352=", b"353="),
    (b"354=", b"355="),
    (b"356=", b"357="),
    (b"358=", b"359="),
    (b"360=", b"361="),
    (b"362=", b"363="),
    (b"364=", b"365="),
    (b"445=", b"446="),
    (b"618=", b"619="),
    (b"621=", b"622="),
    (b"1401=", b"1402="),
    (b"1403=", b"1404="),
];

/// R-P0-13 filter: true when a length field may promise more bytes than follow its data field,
/// the input that makes `QuickFIX` read past the end of the message.
///
/// A data field moves `QuickFIX`'s parse position by its length, not to the next SOH, so this
/// looks at every place a tag could start, not only after SOH: it may skip more than it must.
#[must_use]
pub fn overreads_data_field(data: &[u8]) -> bool {
    DATA_FIELDS.iter().any(|&(length_tag, data_tag)| {
        // The last data field leaves the fewest bytes; every length before it may be used.
        let Some(value_start) = ends_of(data, data_tag).last() else {
            return false;
        };
        let rest = data.len().saturating_sub(value_start);
        ends_of(data, length_tag)
            .take_while(|&end| end < value_start)
            .any(|end| {
                data.get(end..)
                    .is_some_and(|value| leading_number(value) > rest)
            })
    })
}

/// Where each occurrence of `tag` in `data` ends.
fn ends_of<'a>(data: &'a [u8], tag: &'a [u8]) -> impl Iterator<Item = usize> + 'a {
    data.windows(tag.len())
        .enumerate()
        .filter(move |&(_, window)| window == tag)
        .map(move |(start, _)| start.saturating_add(tag.len()))
}

/// The number the digits at the start of `value` spell, saturated; 0 without one (`QuickFIX`
/// refuses a negative length before reading anything).
fn leading_number(value: &[u8]) -> usize {
    value
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .fold(0_usize, |number, &digit| {
            number
                .saturating_mul(10)
                .saturating_add(usize::from(digit.saturating_sub(b'0')))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overreads_data_field_returns_true_when_95_exceeds_the_rest() {
        assert!(overreads_data_field(b"8=FIX.4.4\x0195=100000\x0196=x\x01"));
        assert!(overreads_data_field(b"95=3\x0196=x\x01"));
        assert!(overreads_data_field(b"95=5\x0158=aaaaaa\x0196=x\x01"));
        assert!(overreads_data_field(
            b"93=99999999999999999999999\x0189=x\x01"
        ));
    }

    #[test]
    fn overreads_data_field_returns_true_for_every_known_crash() {
        for crash in [
            &include_bytes!("../known_crashes/r-p0-13-95-100000")[..],
            include_bytes!("../known_crashes/r-p0-13-95-100000000"),
            include_bytes!("../known_crashes/r-p0-13-95-2000000000"),
            include_bytes!("../known_crashes/r-p0-13-fuzzer-first-find"),
        ] {
            assert!(overreads_data_field(crash));
        }
    }

    #[test]
    fn overreads_data_field_returns_false_when_lengths_fit() {
        assert!(!overreads_data_field(b""));
        assert!(!overreads_data_field(b"95=1\x0196=x\x01"));
        assert!(!overreads_data_field(b"95=2\x0196=x\x01"));
        assert!(!overreads_data_field(b"95=-50\x0196=x\x01"));
        assert!(!overreads_data_field(b"95=+9\x0196=x\x01"));
        assert!(!overreads_data_field(b"38=100000\x0158=x\x01"));
        assert!(!overreads_data_field(b"96=x\x0195=100000\x01"));
    }

    #[test]
    fn spec_load_returns_dictionary_for_every_spec() -> Result<(), QuickFixError> {
        for spec in [Spec::Fix44, Spec::Fix44Krx, Spec::Fixt11Fix50] {
            spec.load()?;
        }
        Ok(())
    }
}
