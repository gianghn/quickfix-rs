# Runbook: sequence numbers out of line with the counterparty

For the KRX gateway's operators. The Rust calls are on `quickfix::Session` (look the session up
with `Session::lookup`).

## Symptom

The logon is rejected, or the session logs out right after logon, with a text such as
`MsgSeqNum too low, expecting N but received M`, or the counterparty sends a ResendRequest (35=2)
for messages that are not in our store.

## First: read both numbers

- Ours: `get_expected_sender_num()` (the next number we send) and `get_expected_target_num()`
  (the next number we expect).
- Theirs: from the reject text, the counterparty's operations desk, or their logon (34=).

Write both down before changing anything.

## Fix: set the numbers, don't reset

1. Log the session out (`Session::logout`), and wait until `is_logged_on()` is `false`.
   The setters below take no session lock in QuickFIX: never call them while logged on.
2. Set what the counterparty agreed:
   - `set_next_sender_msg_seq_num(n)`: the number our next message carries.
   - `set_next_target_msg_seq_num(n)`: the number we expect on their next message.
   Both reject 0 and values above `i32::MAX`.
3. Log on (`Session::logon`) and check the logon is accepted.

## Why not `Session::reset`

`reset` wipes the message store: both numbers go back to 1 and every stored outbound message is
gone. The counterparty is not told. Unless it resets at the same moment (a coordinated reset,
usually at the start of the trading day with 141=Y), the next logon fails, and a later
ResendRequest gets gap fills instead of the orders. Use it only for that agreed daily reset.

## Slow stops and resets

`Initiator::stop` and `Acceptor::stop` send a Logout and wait **up to 10 s** for the answer
(QuickFIX's own wait), so a stop that takes about 11 s is normal when the peer is slow to answer.
`reset` and the Logout need the session lock, which every `send_to_target` takes too. Threads that
send in a tight loop without pausing can starve the session's own thread: the 30 s stall the
go-live chaos tests saw inside `reset` (R-P1-7) is consistent with this. In the reset chaos test
(go-live phase 25), 30 seeded runs at about 5 000 sends/s per thread went through 375 resets with
no action over 10 s. With two threads sending flat out, stops ran 10–12 s.

If a stop or a reset hangs longer than 30 s, take thread stacks (`gdb -p PID -batch
-ex 'thread apply all bt'`) before restarting, and attach them to R-P1-7. One such hang is open:
`Initiator::stop` waited more than 60 s to join a socket thread that was still reading a live
connection (go-live phase 25 handover, a C++ fork item).
