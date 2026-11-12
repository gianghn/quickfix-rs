# Gateway handover: moving `krx_gateway_rust` to quickfix-rs 0.3.0

For the team that owns `enCapital/krx_gateway_rust`. What the gateway must change to run on this
fork's `krx/main` (version 0.3.0), and what it must still guard against because the fixes are not in
the library yet. Written 12 November 2026; the item ids (R-P0-n …) are those of
`KRX_GOLIVE_PLAN.md`, the API changes per phase are in `KRX_FORK_NOTES.md` ("Go-live fixes on
`krx/main`") and `quickfix/CHANGELOG.md` (0.3.0).

## 1. Take the new version

- [ ] Depend on `gianghn/quickfix-rs` `krx/main` pinned by commit (`rev = "…"`), all crates at
      `0.3.0`; check out submodules recursively before building (the build no longer fetches them).
- [ ] Drop the `experimental-multi-thread-support` feature from `Cargo.toml` (removed upstream).
- [ ] Ask for `FixSocketServerKind::MultiThreaded` in `src/port_fix/runner.rs`,
      `tests/support/src/connection_safety.rs` and `tests/support/src/checker.rs`. `SingleThreaded`
      is deprecated: QuickFIX's plain socket classes crash in `stop()` (R-P0-8).
- [ ] `cargo deny check` and `cargo audit` on the gateway's own `Cargo.lock` in CI: it, not this
      repo's lock, decides the versions that ship.

## 2. Fix what no longer compiles

The compiler finds all of these; the table in `KRX_FORK_NOTES.md` has one line per phase.

- [ ] App and log types are `Send + Sync` (R-P0-4). A `Cell`/`RefCell` field becomes an atomic or
      a `Mutex`.
- [ ] `on_msg_to_admin` / `on_msg_to_app` take `&mut MessageMut<'_>`; helpers typed `&mut Header`,
      `&Group` take `HeaderMut`, `GroupMut` (R-P0-3). A callback message can no longer be moved out
      with `mem::take`/`swap`: copy what you keep.
- [ ] `Message::from_fix_message_t` is `unsafe` (R-P0-2); state where the pointer comes from in a
      `SAFETY:` comment, and check every use in `src/port_fix`.
- [ ] Generated messages: `X::from(msg)` → `X::try_from(msg)?`; on inbound messages use
      `try_get_*` (they return `FieldError`, never panic) (R-P0-9).
- [ ] `MsgFromAppError::IncorrectTagValue` → `IncorrectTagValue { tag }` (likewise `FieldNotFound`,
      `IncorrectDataFormat`; `RejectLogon { text }`); seqnum APIs take and return `u64`/`Result`.

## 3. Phase 0 rules (still needed with 0.3.0)

- [ ] `panic = "abort"` in `[profile.release]` and `[profile.dev]`, and a supervisor that restarts
      the process. The library now aborts on a callback panic itself (R-P0-1); the peer resends what
      was not handled.
- [ ] Refuse new orders unless the session `is_logged_on()`. In `on_msg_to_app` return `DoNotSend`
      for resent NewOrderSingle / Cancel / Replace older than N seconds, and report them to the OMS
      as not sent. Reason: `send_to_target` still returns `Ok` while the session is down and the
      order goes out later as a PossDup (R-P0-12; needs a C++ fix).
- [ ] Prices from a decimal or integer-tick type, written with `set_field(44, &str)` (R-P0-10);
      `f64` NaN/inf are now refused, but `0.1 + 0.2` still prints badly.
- [ ] Stop and join every sender thread before `stop()` or dropping an initiator/acceptor. A
      racing send now gets `SessionNotFound` instead of a crash (R-P0-6), but it is still lost.
- [ ] Production `.ini`, versioned in the gateway, with a test that loads it through
      `SessionSettings::try_from_path`: `PersistMessages=Y`, `ResetOn*=N`, `SocketNodelay=Y`,
      `TimestampPrecision`, `HeartBtInt`, `CheckLatency`/`MaxLatency` per the KRX spec (R-P1-8).
- [ ] Dictionary (R-P0-11): confirm which XML is loaded as `DataDictionary=`; set
      `AllowUnknownMsgFields=Y`, `ValidateUserDefinedFields=N`, `ValidateFieldsOutOfOrder=N`;
      alert on every outbound 35=3 and 35=j. FIX44KRX.xml is not yet checked against the member
      spec (it requires 128/129 in every header and has no Text(58) on 35=8).
- [ ] `Session::reset` only behind an operator command with confirmation; operators read
      `doc/RUNBOOK_SEQNUM.md` (it wipes the store; prefer `set_next_*`).
- [ ] Audit log: override `LogCallback::on_*_bytes` to journal exact wire bytes (R-P1-3).

## 4. Release build (RHEL/Rocky Linux 9)

- [ ] Release profile `debug = "line-tables-only"`; the release job runs
      `scripts/split-debuginfo.sh <binary>`, ships the stripped binary and archives `<binary>.debug`
      (`KRX_FORK_NOTES.md`, "Shipping a release binary").
- [ ] Build on Rocky Linux 9, the production distro; this repo's CI gets a Rocky 9 job too.

## 5. Known open risks the gateway carries until the C++ fork is fixed

| Risk | What happens | Gateway mitigation |
|---|---|---|
| R-P0-13 data-length over-read (QuickFIX #779) | one message with a huge or negative 95/96 length crashes the process, before logon, on the acceptor and the initiator side | expose no acceptor to untrusted networks; the exchange link is trusted but not immune; supervisor restart and alert on abort |
| R-P0-12 send while down | `Ok` returned, sent later as PossDup | the logged-on check and stale-resend `DoNotSend` above |
| C++ data races (`FIX::Mutex`, `SessionState` flags and times) | rare, timing-dependent misbehaviour under load | none in the gateway; tracked for the C++ fork |

## 6. Done when

- [ ] The gateway's own tests pass on 0.3.0 with `MultiThreaded`, including `test_fd_double_close`,
      `test_partial_send` and `fix_connection_safety.feature`.
- [ ] A UAT drill: kill the process mid-flow (abort), restart, and check the exchange resend
      delivers the lost message; disconnect, send, reconnect and check the stale order is
      gap-filled and the OMS told.
- [ ] A replay of the certification/UAT capture through the production `.ini` and dictionary
      gives zero outbound 35=3.
