//! A FIX peer written by hand on a TCP socket, for tests that need exact wire bytes or a peer
//! that misbehaves (never answers a Logout, sends bytes QuickFIX would not).

use std::{
    io::{Read, Write},
    net::TcpStream,
    time::Duration,
};

const SOH: u8 = 1;

/// Frames `fields` as FIX 4.4 wire bytes with a correct BodyLength and CheckSum.
pub fn wire(fields: &[(i32, &[u8])]) -> Vec<u8> {
    let mut body = Vec::new();
    for (tag, value) in fields {
        body.extend_from_slice(format!("{tag}=").as_bytes());
        body.extend_from_slice(value);
        body.push(SOH);
    }
    let mut out = format!("8=FIX.4.4\x019={}\x01", body.len()).into_bytes();
    out.extend_from_slice(&body);
    let sum = out.iter().map(|b| u32::from(*b)).sum::<u32>() % 256;
    out.extend_from_slice(format!("10={sum:03}\x01").as_bytes());
    out
}

/// Standard header of a message from SENDER to RECEIVER.
pub fn header(msg_type: &'static [u8], seq: &'static [u8]) -> Vec<(i32, &'static [u8])> {
    vec![
        (35, msg_type),
        (49, b"SENDER"),
        (56, b"RECEIVER"),
        (34, seq),
        (52, b"20261111-02:15:00.000"),
    ]
}

pub fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

/// Connects to an acceptor on `port` as SENDER and logs on; returns the open socket.
pub fn logon(port: u16) -> TcpStream {
    let mut peer = TcpStream::connect(("127.0.0.1", port)).expect("acceptor listens");
    peer.set_read_timeout(Some(Duration::from_secs(5)))
        .expect("read timeout");
    let mut logon = header(b"A", b"1");
    logon.extend([(98, &b"0"[..]), (108, b"30")]);
    peer.write_all(&wire(&logon)).expect("logon sent");
    let mut reply = [0u8; 512];
    let n = peer.read(&mut reply).expect("logon answered");
    assert!(contains(&reply[..n], b"35=A\x01"), "no logon reply");
    peer
}
