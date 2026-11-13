//! Phase 19 (D20): a test builds the message a callback is given, `Message::as_message_mut`, and
//! calls `on_msg_to_app` / `on_msg_to_admin` directly, without a session.

use quickfix::*;

/// Tags a gateway's outbound callback would stamp.
const TEXT: i32 = 58;
const SENDER_SUB_ID: i32 = 50;
const NO_PARTY_IDS: i32 = 453;
const PARTY_ID: i32 = 448;
const SYMBOL: i32 = 55;

struct Stamper;

impl ApplicationCallback for Stamper {
    fn on_msg_to_admin(&self, msg: &mut MessageMut<'_>, _session: &SessionId) {
        msg.with_header_mut(|header| header.set_field(SENDER_SUB_ID, "DESK1"))
            .expect("header field set");
    }

    fn on_msg_to_app(
        &self,
        msg: &mut MessageMut<'_>,
        _session: &SessionId,
    ) -> Result<(), MsgToAppError> {
        if msg.get_field(SYMBOL).as_deref() == Some("HALTED") {
            return Err(MsgToAppError::DoNotSend);
        }
        msg.set_field(TEXT, "stamped").expect("body field set");
        let mut party = Group::try_new(NO_PARTY_IDS, PARTY_ID).expect("group");
        party.set_field(PARTY_ID, "FIRM").expect("group field set");
        msg.add_group(&party).expect("group added");
        Ok(())
    }
}

fn session() -> SessionId {
    SessionId::try_new("FIX.4.4", "SENDER", "RECEIVER", "").expect("session id")
}

#[test]
fn on_msg_to_app_changes_reach_the_owned_message() -> Result<(), QuickFixError> {
    let mut msg = Message::new();
    msg.set_field(SYMBOL, "VNM")?;

    let sent = Stamper.on_msg_to_app(&mut msg.as_message_mut(), &session());

    assert!(sent.is_ok());
    assert_eq!(msg.get_field(TEXT).as_deref(), Some("stamped"));
    let party = msg
        .clone_group(1, NO_PARTY_IDS)
        .expect("group on the message");
    assert_eq!(party.get_field(PARTY_ID).as_deref(), Some("FIRM"));
    Ok(())
}

#[test]
fn on_msg_to_admin_header_changes_reach_the_owned_message() {
    let mut msg = Message::new();

    Stamper.on_msg_to_admin(&mut msg.as_message_mut(), &session());

    let sub_id = msg.with_header(|header| header.get_field(SENDER_SUB_ID));
    assert_eq!(sub_id.as_deref(), Some("DESK1"));
}

#[test]
fn on_msg_to_app_do_not_send_reaches_the_test() -> Result<(), QuickFixError> {
    let mut msg = Message::new();
    msg.set_field(SYMBOL, "HALTED")?;

    let sent = Stamper.on_msg_to_app(&mut msg.as_message_mut(), &session());

    assert!(matches!(sent, Err(MsgToAppError::DoNotSend)));
    assert_eq!(msg.get_field(TEXT), None);
    Ok(())
}
