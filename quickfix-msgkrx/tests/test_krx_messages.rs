//! KRX-shaped messages through the KRX dictionary (FIX44KRX.xml), the way the gateway reads them.
//!
//! Red tests carry `#[ignore = "red until <item>"]` (KRX_BUILD_PLAN.md, D4); the phase that fixes
//! the item removes the attribute.

use quickfix::{DataDictionary, FieldMap, Message};
use quickfix_msgkrx::field_types::Side;
use quickfix_msgkrx::{field_id, NewOrderSingle, Quote};

const SOH: char = '\u{1}';
const KRX_DICTIONARY: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/src/FIX44KRX.xml");

fn dictionary() -> DataDictionary {
    DataDictionary::try_from_path(KRX_DICTIONARY).expect("FIX44KRX.xml loads")
}

/// Frames `fields` as FIX 4.4 wire text with a correct BodyLength (9) and CheckSum (10).
fn wire(fields: &[(i32, &str)]) -> String {
    let body: String = fields
        .iter()
        .map(|(t, v)| format!("{t}={v}{SOH}"))
        .collect();
    let head = format!("8=FIX.4.4{SOH}9={}{SOH}", body.len());
    let sum = head.bytes().chain(body.bytes()).map(u32::from).sum::<u32>() % 256;
    format!("{head}{body}10={sum:03}{SOH}")
}

/// A KRX header (128/129 are required there) for `msg_type`, then `extra` header fields, then `body`.
fn krx_message(msg_type: &str, extra: &[(i32, &str)], body: &[(i32, &str)]) -> String {
    let mut fields = vec![
        (35, msg_type),
        (49, "MEMBER01"),
        (56, "KRX"),
        (128, "HOSE"),
        (129, "TRADING"),
        (34, "7"),
        (52, "20261111-02:15:00.123"),
    ];
    fields.extend_from_slice(extra);
    fields.extend_from_slice(body);
    wire(&fields)
}

const NEW_ORDER_SINGLE: &[(i32, &str)] = &[
    (11, "ORD-0001"),
    (1, "058C000123"),
    (21, "1"),
    (55, "VNM"),
    (54, "1"),
    (60, "20261111-02:15:00.120"),
    (38, "1000"),
    (40, "2"),
    (44, "61500"),
    (75, "20261111"),
    (421, "VN"),
];

/// A Quote (35=S) with two parties, the second with two sub-IDs (nested group).
const QUOTE: &[(i32, &str)] = &[
    (117, "Q-1"),
    (1166, "QM-1"),
    (537, "1"),
    (1171, "N"),
    (453, "2"),
    (448, "FIRM-A"),
    (447, "D"),
    (452, "1"),
    (448, "FIRM-B"),
    (447, "D"),
    (452, "17"),
    (802, "2"),
    (523, "TRADER-1"),
    (803, "2"),
    (523, "DESK-9"),
    (803, "10"),
    (55, "VNM"),
    (54, "1"),
    (1, "058C000123"),
    (581, "1"),
    (60, "20261111-02:15:00.120"),
    (40, "2"),
    (75, "20261111"),
    (421, "VN"),
    (6000, "1"),
];

fn build(text: &str) -> Message {
    dictionary()
        .try_build_message(text)
        .expect("message parses")
}

#[test]
fn try_from_path_loads_krx_dictionary() {
    assert!(DataDictionary::try_from_path(KRX_DICTIONARY).is_ok());
}

#[test]
fn to_fix_string_keeps_every_field_of_new_order_single() {
    // QuickFIX writes fields back sorted by tag, so compare field by field, not byte for byte.
    let out = build(&krx_message("D", &[], NEW_ORDER_SINGLE))
        .to_fix_string()
        .expect("message serialises");
    for (tag, value) in NEW_ORDER_SINGLE {
        assert!(
            out.contains(&format!("{SOH}{tag}={value}{SOH}")),
            "{tag}={value} missing in {out:?}"
        );
    }
    assert!(out.starts_with(&format!("8=FIX.4.4{SOH}9=183{SOH}35=D{SOH}")));
    assert!(out.ends_with(&format!("10=127{SOH}")));
}

#[test]
fn get_field_reads_krx_header_and_body() {
    let msg = build(&krx_message("D", &[], NEW_ORDER_SINGLE));
    let header = |tag| msg.with_header(|h| h.get_field(tag));
    assert_eq!(
        header(field_id::DELIVER_TO_COMP_ID).as_deref(),
        Some("HOSE")
    );
    assert_eq!(
        header(field_id::DELIVER_TO_SUB_ID).as_deref(),
        Some("TRADING")
    );
    assert_eq!(header(field_id::MSG_SEQ_NUM).as_deref(), Some("7"));
    assert_eq!(msg.get_field(field_id::PRICE).as_deref(), Some("61500"));
}

#[test]
fn new_order_single_getters_read_required_fields() {
    let order = NewOrderSingle::from(build(&krx_message("D", &[], NEW_ORDER_SINGLE)));
    assert_eq!(order.get_cl_ord_id(), "ORD-0001");
    assert_eq!(order.get_symbol(), "VNM");
    assert_eq!(order.get_side(), Side::Buy);
    assert_eq!(order.get_account(), "058C000123");
}

#[test]
fn with_group_reads_parties_of_quote() {
    let msg = build(&krx_message("S", &[], QUOTE));
    let party = |i| {
        msg.with_group(i, field_id::NO_PARTY_I_DS, |g| {
            g.get_field(field_id::PARTY_ID)
        })
    };
    assert_eq!(party(1).flatten().as_deref(), Some("FIRM-A"));
    assert_eq!(party(2).flatten().as_deref(), Some("FIRM-B"));
    assert_eq!(party(3), None);
}

#[test]
fn iter_no_party_i_ds_reads_parties_and_nested_sub_ids() {
    let quote = Quote::from(build(&krx_message("S", &[], QUOTE)));
    assert_eq!(quote.no_party_i_ds_len(), 2);

    let parties: Vec<_> = quote.iter_no_party_i_ds().collect();
    let ids: Vec<_> = parties.iter().map(|p| p.get_party_id()).collect();
    assert_eq!(
        ids,
        [Some("FIRM-A".to_string()), Some("FIRM-B".to_string())]
    );

    let subs: Vec<_> = parties[1]
        .iter_no_party_sub_i_ds()
        .map(|s| s.get_party_sub_id())
        .collect();
    assert_eq!(
        subs,
        [Some("TRADER-1".to_string()), Some("DESK-9".to_string())]
    );
    assert_eq!(parties[0].iter_no_party_sub_i_ds().count(), 0);
}

#[test]
fn get_poss_dup_flag_returns_none_when_43_is_absent() {
    let mut order = NewOrderSingle::from(build(&krx_message("D", &[], NEW_ORDER_SINGLE)));
    assert_eq!(order.header().get_poss_dup_flag(), None);
}

#[test]
#[ignore = "red until R-P0-9: FIX booleans are Y/N, the generator parses them with str::parse::<bool>"]
fn get_poss_dup_flag_returns_true_when_43_is_y() {
    let text = krx_message(
        "D",
        &[(43, "Y"), (122, "20261111-02:14:00.000")],
        NEW_ORDER_SINGLE,
    );
    let mut order = NewOrderSingle::from(build(&text));
    assert_eq!(order.header().get_poss_dup_flag(), Some(true));
}
