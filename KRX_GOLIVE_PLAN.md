# KRX go-live plan for this fork

What we must change in `gianghn/quickfix-rs` (krx/main) before DNSE's HOSE/KRX gateway
(`enCapital/krx_gateway_rust`) goes live on it.

Source: the go-live review of 10 November 2026, at head `1b42bc5`, libquickfix `33b12daf`
([report](https://github.com/gianghn/ClaudeResearch/blob/main/quickfix-rs-golive/report/quickfix-rs-golive.md),
[swipe quiz](https://gianghn.github.io/ClaudeResearch/quickfix-rs-golive/),
[harness](https://github.com/gianghn/ClaudeResearch/tree/main/quickfix-rs-golive/harness)).
Item ids (R-P0-n, R-P1-n, R-P2-n) are the report's; C++ P0-n refers to the C++ review
([quickfix-golive](https://github.com/gianghn/ClaudeResearch/tree/main/quickfix-golive)).
File:line references are at `1b42bc5`.

**Verdict: not ready.** The happy path is clean (1.62 M orders in a chaos run with no loss,
duplicates or reordering; 0 RustSec advisories). The failure paths are not: 13 P0, 8 P1, 7 P2.

**Status, 12 November 2026:** `krx/main` at 0.3.0 (`4dcb99f`) carries the fixes for 10 of the
13 P0s, 5 of the 8 P1s and 3 of the 7 P2s ([x] below; details, tests and handovers in
`KRX_BUILD_PLAN.md`). Still open in this repo: R-P0-13's regression test, the rest of R-P1-5 and
the R-P1-7 chaos run. Waiting outside it: R-P0-11 (member spec), R-P0-12 and R-P0-13 (C++ fork),
R-P2-3/4/5/6, Phase 0 (gateway) and Phase 4 (evidence).

**Update, 13 November 2026:** the C++ fork fixed R-P0-13, its half of R-P0-12 and the
`FIX::Mutex`/`SessionState` races; phase 16 pins it (`11703b2a`), so 11 of 13 P0s are done.
R-P0-12 needs phase 17 here (`send_to_target` must surface C++'s `false`). The CI TSan job was
report-only (D6) while every report (202 in the last run) had C++ frames on both sides
(`FIX::Mutex`, `SessionState` flags and times); on the new pin it reports 0, and phase 18 makes
it blocking (D18).

## How the work is split

| Who | What |
|---|---|
| Gateway (`krx_gateway_rust`) | Phase 0 mitigations: settings and rules, no library release needed |
| This fork (`quickfix-rs` krx/main) | Phases 1 to 3: binding, generator, KRX crate, build and CI |
| C++ fork (`gianghn/quickfix` krx/main) | C++ P0s; reaches the gateway only through a submodule bump here |
| Everyone | Phase 4: evidence on one certified pair (C++ SHA, quickfix-rs SHA) |

Rule for every item: the fix lands with the test named next to it, and the harness
programs that reproduced the bug become permanent regression tests in this repo.

## Phase 0: gateway mitigations (days, outside this repo)

Listed here because they decide which P0s are reachable today. Track them in the gateway.

- [ ] `panic = "abort"` in `[profile.release]` and `[profile.dev]`, run under a supervisor that restarts it (R-P0-1).
- [ ] Ask for `FixSocketServerKind::MultiThreaded` in `src/port_fix/runner.rs`,
      `tests/support/src/connection_safety.rs`, `tests/support/src/checker.rs`; drop the removed
      `experimental-multi-thread-support` feature; depend on 0.2.2 (R-P0-8).
- [ ] Compile-time assertion that the app and log types are `Send + Sync` (R-P0-4):
      `const _: fn() = || { fn f<T: Send + Sync>() {} f::<GatewayApp>(); f::<GatewayLog>(); };`
- [ ] Quiesce and join every sender before `stop()`, drop or recreate of an Initiator/Acceptor (R-P0-6).
- [ ] Refuse new orders unless `is_logged_on()`; in `on_msg_to_app` return `DoNotSend` for resent
      NewOrderSingle/Cancel/Replace older than N seconds and tell the OMS "not sent" (R-P0-12).
- [ ] No generated getters inside callbacks: read required tags and 43/97 with raw `get_field`,
      map failures to an explicit `MsgFromAppError` (R-P0-9).
- [ ] Prices out of a decimal or i64-tick type, written with `set_field(44, &str)` (R-P0-10).
- [ ] Grep for `from_fix_message_t`, `get_fix_message_t`, `mem::take/replace/swap` on callback
      messages; add clippy `disallowed_methods` for them (R-P0-2, R-P0-3).
- [ ] Confirm which XML the gateway loads as `DataDictionary=`; set `AllowUnknownMsgFields=Y`,
      `ValidateUserDefinedFields=N`, `ValidateFieldsOutOfOrder=N`; alert on every outbound 35=3/35=j (R-P0-11).
- [ ] `cargo audit` and `cargo deny check` on the gateway's own `Cargo.lock` (R-P1-6).
- [ ] Versioned production `.ini` (R-P1-8): `PersistMessages=Y` explicit, `ResetOn*=N`, `SocketNodelay=Y`,
      `TimestampPrecision`, `HeartBtInt`, `CheckLatency/MaxLatency` per KRX spec; a unit test loads it
      through `SessionSettings::try_from_path` and asserts each key.

## Phase 1: crashers and silent loss (sprint 1)

Ends with the first new submodule pin (C++ P0-1/2/3/7/11 fixed) and ASan/TSan CI jobs green here.

- [x] **R-P0-1 Panic in a callback is reported as success.** 3,504 of 3,504 panicked inbound messages
      were lost; `to_app` panics were sent; a panicking `on_logon` left the session up.
      Where: `quickfix/src/application.rs:47-53, 155-238`, `quickfix/src/log_factory.rs:56-96`.
      Fix: `Err(_) => std::process::abort()` in every callback (not `0`), so it no longer depends on the
      consumer's profile; update the panic statement in `doc/DEV_NOTES.md`.
      Test: two-session FileStore death test injecting panics in `from_app`, `to_app`, `to_admin`,
      `on_logon` and each log callback: process aborts, next target seqnum did not advance, the peer's
      resend delivers the message after restart. (Also the first test of `panic = "abort"`: still untested.)
- [x] **R-P0-5 `MsgFromAppError::UnsupportedMessageType` (-6) is dropped silently.**
      Where: `quickfix-ffi/quickfix-bind/src/quickfix_bind.cpp:285-301`, `application.rs:72-80`.
      Fix: `case CALLBACK_RESULT_UNSUPPORTED_MESSAGE_TYPE: throw UnsupportedMessageType();` and a
      fail-closed `default:` for unknown non-zero codes in both switches.
      Test: end-to-end test per `MsgFromAppError`, `MsgFromAdminError` and `DoNotSend` variant; for -6
      the peer gets 35=j with 380=3.
- [x] **R-P0-7 `clone_group` reads past the heap** (every generated group getter).
      Where: `quickfix_bind.cpp:986-1021` (`FixMessage_copyGroup`, `FixHeader_copyGroup`,
      `FixGroup_copyGroup`, `FixMessage_getGroupRef`); generator `quickfix-msg-gen/src/lib.rs:724-748`.
      Fix: build `new Group(tag, delim)` and assign the `FieldMap`; `getGroupRef` returns `FieldMap*`;
      generator iterates with `with_group` (borrow) instead of cloning.
      Test: `test_message_group` clean under ASan; ASan test reading every group of a captured KRX message.
- [x] **R-P0-6 Drop of Initiator/Acceptor while another thread is in `send_to_target` = use-after-free.**
      Where: `quickfix/src/session.rs:15-18`, `initiator.rs`, `acceptor.rs:116-126`.
      Fix: process-wide `RwLock` gate (sends take read, `Drop` takes write before `*_delete`), or send
      only through a borrow of the container.
      Test: harness chaos "drop+recreate initiator while sending" under ASan, 50 seeded runs, zero reports.
- [x] **R-P0-8 `SingleThreaded` `stop()` segfaults; our tests use it** (6 of 10 `cargo test` runs on loaded Linux).
      Where: `quickfix/src/lib.rs:279-316` (default kind), `lib.rs:85-90` (doc example),
      `quickfix/tests/utils/checker.rs:41-58`, `test_send_receive.rs:66`, `test_session.rs`, `test_socket.rs`.
      Fix: tests on MultiThreaded (or both kinds); wait for both sides to log on in `checker.rs:58` and
      `test_send_receive.rs:66`; make `MultiThreaded` the default or `#[deprecated]` `SingleThreaded`;
      settle the pending threading-flag decision in `KRX_FORK_NOTES.md`.
      Test: `run_tests_loop.sh 50` on Linux under load, zero failures.
- [x] **R-P0-2 Safe `Message::from_fix_message_t` adopts any raw pointer** (fork-added, no tests).
      Where: `quickfix/src/message.rs:36-44`, Drop at `:277-281`.
      Fix: `unsafe fn` with a written ownership contract, or `into_raw(self)` + `unsafe from_raw`;
      reject null; copy with `FixMessage_copy` where a value is enough.
      Test: `compile_fail` doctest for the safe call; ASan test of new → `into_raw` → `from_raw` → drop.
- [x] **R-P0-4 `ApplicationCallback` and `LogCallback` lack `Send + Sync`.**
      Where: `application.rs:83-87, 128-143`, `log_factory.rs:18-28`.
      Fix: `pub trait ApplicationCallback: Send + Sync`, same for `LogCallback` (breaking, and correct).
      Test: `compile_fail` doctest with a `Cell` member; TSan run with one app shared by two MultiThreaded sessions.
- [x] **R-P0-13 C++ data-length over-read (#779) kills the process with one raw Logon** (`95=100000000`).
      Done: C++ fork P0-1, pinned in go-live phase 16 (`11703b2a`); all 20 data-length tests green under ASan.
      Fix: in the C++ fork (cherry-pick #779), then bump the submodule here.
      Test (this repo): integration test with 95 = 1e5 / 1e8 / 2e9 / -50 over raw TCP and through
      `DataDictionary::try_build_message`, acceptor and initiator, under ASan in CI: no crash.

## Phase 2: API soundness and the KRX crate (sprint 2)

- [x] **R-P0-3 `&mut Message/Header/Trailer/Group` over borrowed C++ objects can be moved out** (`mem::take`).
      Where: `application.rs:182-206`, `message.rs:144-224`.
      Fix: borrowed view types (`MessageMut<'a>`, `HeaderMut<'a>`, `TrailerMut<'a>`, `GroupMut<'a>`)
      implementing `FieldMap` without `Drop`/`Default`, as the generator already does (`quickfix-msg-gen/src/lib.rs:398-418`).
      Test: `repro swap_header` and `swap_to_app` become `compile_fail` doctests; ASan callback suite clean.
- [x] **R-P0-9 Generated KRX getters panic; BOOLEAN fields never parse** (142 `.expect` getters,
      `From<Message>` asserts MsgType, `"Y".parse::<bool>()` fails for all 31 bool fields incl. 43/97).
      Where: `quickfix-msg-gen/src/lib.rs:430-440, 540-546, 656-666`.
      Fix: generate `try_get_<field>() -> Result<T, FieldError>` and `TryFrom<Message>`; FIX Y/N bool parser.
      Test: new `quickfix-msgkrx/tests` parsing captured KRX 35=D and 35=8 incl. `43=Y`, unknown enum
      values (`150=C`) and missing required tags: no panic, correct `Result`s.
- [x] **R-P0-10 Prices and quantities are `f64`** (`44=0.30000000000000004`, `44=NaN` accepted).
      Where: `quickfix-msg-gen/src/lib.rs:257-300`, `quickfix/src/lib.rs:203-216`.
      Fix: reject NaN/inf in `IntoFixValue for f64`; optionally emit a decimal type from the generator.
      Test: property test over 10/50/100 VND ticks and derivative/bond decimals: wire string equals decimal string.
- [ ] **R-P0-11 FIX44KRX.xml risks.** 128/129 required in every header (admin too), no Text(58),
      SecondaryOrderID or Currency on 35=8, ExecType only 0,4,5,8,D,F,L, header drops 369.
      Where: `quickfix-msgkrx/src/FIX44KRX.xml:8-30, 237`.
      Fix: check every message, field, requiredness and enum against the HOSE/KRX member spec; the
      runtime dictionary must be a superset of the spec.
      Test: replay the full certification/UAT capture: zero outbound 35=3; a 35=8 with 58 and `150=C` reaches `from_app`.
- [ ] **R-P0-12 `send_to_target` returns `Ok` while the session is down** (1.23 M of 1.62 M chaos orders
      went out later as PossDup). Where: `session.rs:13-18, 58-61`, `quickfix_bind.cpp:1069-1077`.
      Fix: once C++ P0-6 makes the bool truthful, return `SendOutcome { Sent, Stored, Dropped }`.
      Test: send during disconnect/reconnect; stale order is gap-filled, never reaches the simulator.
- [x] **R-P1-1 Seqnum APIs return error codes as seqnums and take any `i32`** (fork-added).
      Where: `session.rs:73-89`, `quickfix_bind.cpp:1131-1139`.
      Fix: `Result<u64, QuickFixError>`; take `u64`, reject < 1; take the session lock or document "logged out only".
      Test: MultiThreaded + FileStore: set 100 → next 34=100, persists over restart; `set(0)` → `Err`;
      4-thread send race while setting shows no duplicate or skipped 34.
- [x] **R-P1-2 Stale thread-local last-error; error buffer leaks per thread.**
      Where: `quickfix_bind.cpp:191-219`, `error.rs:141-199`, `session.rs:36-44`.
      Fix: clear the code on entry to every wrapper; `lookup` → `None` maps to `SessionNotFound`; `thread_local std::string`.
      Test: `repro stale_error` asserts `SessionNotFound`; LSan clean.
- [x] **R-P1-3 Log callback replaces non-UTF-8 text with a placeholder and truncates at NUL** (audit-trail holes).
      Where: `quickfix/src/utils.rs:8-19`, `quickfix_bind.cpp:326-342`.
      Fix: pass `(ptr, len)`; callbacks get `&[u8]` or `Cow<str>`; add `get_field_bytes`.
      Test: raw peer sends `58=caf\xE9` and `58=ab\0cd`; log text and `get_field_bytes` match the wire.
- [x] **R-P1-4 `to_fix_bytes`/`to_fix_raw_bytes` zero-fill after an embedded NUL** (`strncpy`).
      Where: `quickfix_bind.cpp:844-859`, `message.rs:55-108`.
      Fix: `memcpy(buffer, value.data(), value.size())`; `to_fix_string` returns `Result`.
      Test: parse `95=3|96=a\0b` and compare `to_fix_bytes()` byte for byte with the wire.
- [ ] **R-P1-7 `Session::reset` is a one-sided reset that wipes the store; one 30 s stall inside it is unexplained.**
      Fix: rustdoc warning and an operator runbook (when to use `set_next_*` instead).
      Test: 30 seeded chaos runs with reset on and a watchdog; any stall over 10 s yields stacks; zero unexplained.

## Phase 3: build, CI and fork hygiene (alongside sprints 1 and 2)

- [ ] **R-P1-5 Non-hermetic, unhardened build.** `quickfix-ffi/build.rs:22-35, 55-74, 131-145`.
      Fix: fail with a clear message instead of `git submodule update` over the network; sccache/ccache only
      by env opt-in; add `-fno-omit-frame-pointer -D_FORTIFY_SOURCE=2 -fstack-protector-strong
      -fstack-clash-protection -ffile-prefix-map`; drop `-w` (`-Wall` visible in CI); pinned container; split debuginfo.
      Test: CI build fails with no submodule and no network; `checksec` shows the flags; two clean builds give identical `libquickfix.a`.
- [x] **R-P1-6 CI gaps** in `.github/workflows/krx_ci.yml`. Add: `cargo build --release --locked`; MSRV 1.86 job;
      `clippy -D warnings` on library crates; `cargo deny check` + `cargo audit`; ASan and TSan jobs (harness
      `build_san.sh` recipe); 20× loop of socket tests; submodule SHA equals the pin; production-matching
      distro image; SSL feature job; actions pinned by SHA.
      Test: all green, and a deliberate `clone_group` regression turns the ASan job red.
- [x] **R-P2-1** `catch (...)` in every wrapper; the SSL stub throws by value, not `new std::logic_error`
      (`quickfix_bind.cpp:28-36, 89-182`). Test: `FixAcceptor_new(…, ssl=1)` in a non-SSL build returns an error code.
- [x] **R-P2-2** Small fixes, one unit test each: `FfiMessageStoreFactory` becomes `unsafe trait`;
      `Dictionary::get` int/double must not return `Ok(0)` on error; null-check `msg` in `FixSession_send`;
      rejects/logouts carry tag and text; `stop(force)` so `Drop` does not block ~10 s; `is_enabled`,
      `refresh`, on-demand TestRequest; socket buffer items wider than `u16`; typed `TimestampPrecision`.
- [ ] **R-P2-3** Fix the plain `SocketAcceptor` close/delete race in the C++ fork (`SocketAcceptor.cpp:110-167`).
      Test: 10-cycle stop loop in SingleThreaded mode, zero crashes.
- [ ] **R-P2-4** Fast-forward fork `main` (stale at `1ee16fe`, 21 behind upstream `cc2b7a5`) and add a scheduled check.
- [ ] **R-P2-5** Bump the submodule to C++ krx/main `25273cc` or record why not (superseded by the sprint 1 pin).
- [ ] **R-P2-6** Upstream the `test_session` fix (`c5f99f3`), then the seqnum API, -6 and `clone_group` patches.
- [x] **R-P2-7** Docs and metadata: drop "ready for production" from the README; fix the `quickfix-msgkrx`
      crate doc ("FIX44.xml"), `authors` and `license`; replace the SharePoint link in FIX44KRX.xml with a
      versioned spec reference.

## Phase 4: evidence on the certified pair

No code; this decides go-live.

- [ ] Port the harness (`qfrs_stress.rs`, `audit-repro`, api probe, fuzz crate) into this repo as tests.
- [ ] Rerun the harness with drop and reset on, echo on (after C++ P0-3), panic injection under abort and the
      data-length frames, under ASan and TSan, 50 seeded runs each.
- [ ] Fuzzing with a byte-level entry point and a FIXT/FIX50 mode (the `&str` entry point missed the known over-read).
- [ ] Multi-hour release-profile soak with latency (only dev-build figures exist: p99 ~114-130 µs loopback).
- [ ] Dictionary replay of the full UAT capture (R-P0-11 / C++ P0-8).
- [ ] Root-cause C++ P0-10 (sequence gap) and the `Session::reset` stall (R-P1-7).
- [ ] HOSE certification on the final pinned (C++ SHA, quickfix-rs SHA) pair, vendored into the gateway as one unit.

## Open questions

- Gateway not yet audited: its `Cargo.lock`, panic profile, use of `from_fix_message_t`/`mem::take`,
  `Sync` of its app and log types, and runtime XML decide which P0s are reachable today.
- `panic = "abort"` is the chosen direction but untested (first R-P0-1 test settles it).
- Echo-on hang (both peers pushing) matches C++ P0-3 but has no stacks yet.
- HOSE/KRX member spec needed to finish R-P0-11.
- Licence: `MIT OR Apache-1.1` is deliberate (LICENSE-APACHE is the QuickFIX licence); acceptance is a legal decision.
