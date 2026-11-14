//! Phase 22: the cost of one header read plus one header write, the pair the gateway makes on
//! every order it sends to the exchange. A measurement, not a check, so it is ignored by default:
//! `cargo test --release -p quickfix --test test_field_cost -- --ignored --nocapture`.

use std::hint::black_box;
use std::time::Instant;

use quickfix::*;

const DELIVER_TO_SUB_ID: i32 = 129;
const CALLS_PER_ROUND: u32 = 1_000_000;
const ROUNDS: usize = 15;

fn one_read_and_write(msg: &mut Message) {
    msg.with_header_mut(|header| {
        let value = header.get_field(DELIVER_TO_SUB_ID);
        black_box(&value);
        header
            .set_field(DELIVER_TO_SUB_ID, "G1")
            .expect("set header field");
    });
}

#[test]
#[ignore = "measurement: run in release with --ignored --nocapture"]
fn header_read_and_write_cost() {
    let mut msg = Message::new();
    msg.with_header_mut(|header| header.set_field(DELIVER_TO_SUB_ID, "G1"))
        .expect("set header field");

    let mut rounds: Vec<f64> = (0..ROUNDS)
        .map(|_| {
            let start = Instant::now();
            for _ in 0..CALLS_PER_ROUND {
                one_read_and_write(black_box(&mut msg));
            }
            start.elapsed().as_secs_f64() * 1e9 / f64::from(CALLS_PER_ROUND)
        })
        .collect();
    rounds.sort_by(f64::total_cmp);

    println!(
        "header get_field + set_field: median {:.1} ns, min {:.1} ns, max {:.1} ns ({ROUNDS} rounds)",
        rounds[ROUNDS / 2],
        rounds[0],
        rounds[ROUNDS - 1]
    );
}
