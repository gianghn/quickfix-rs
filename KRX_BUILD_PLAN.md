# KRX build plan: the go-live items this repo can fix alone

How we build the items of `KRX_GOLIVE_PLAN.md` that need **no change in the C++ fork**
(`gianghn/quickfix`), test first. Item ids (R-P0-n, R-P1-n, R-P2-n) are the go-live plan's.

## 1. Scope

In: sanitizer CI, a test suite for `quickfix-msgkrx`, then every go-live item whose fix lives in
this repo (Rust crates, the generator, and the C++ binding `quickfix-ffi/quickfix-bind`, which is
ours).

Out (each waits on something outside this repo):

| Item | Waits on |
|---|---|
| R-P0-11 FIX44KRX.xml vs the HOSE/KRX member spec | the member spec (owner) |
| R-P0-12 `SendOutcome` | C++ P0-6 (a truthful send result) |
| R-P0-13 data-length over-read | C++ #779 cherry-pick + submodule bump |
| R-P2-3 plain `SocketAcceptor` close race, R-P2-5 submodule bump | C++ fork |
| R-P2-4 fork `main` fast-forward, R-P2-6 upstreaming | owner (outward-facing) |
| Phase 0 gateway mitigations, Phase 4 evidence | gateway repo / certified pair |

Tests for the waiting items may still be written early; they stay out of `krx/main` until the
fix arrives, so CI stays green.

## 2. Ground rules

- **TDD per item.** Write the test named in the go-live plan first, run it, record the red result
  (failing assertion, ASan report, abort, or `compile_fail` that compiles) in the handover note,
  then fix until green. No item is done without its test.
- **No BDD in this repo** (decided 11 Nov 2026): this is an FFI library; its risks are memory,
  threads and wire bytes, which Rust tests, doctests and sanitizers express directly. Business
  rules (refuse orders while logged out, stale resends) get Gherkin in the gateway.
- **Test kinds:** unit and integration tests (`cargo test`), `compile_fail` doctests for
  soundness, death tests (child process, expect `SIGABRT`) for aborts, and the same suite under
  ASan and TSan (`scripts/san.sh`, CI jobs from phase 1).
- **One phase, one branch, one commit, one PR**, stacked: `krx/golive-p<k>` from the previous
  phase's branch, the first from `krx/main`. PRs go into `krx/main` in order.
- Commits stay local until the owner says to push and open the PRs.
- Every phase ends green on: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets`,
  `cargo test --workspace`, the ASan job (from phase 1), and the Local_Claude_Setup `lint.sh`
  as far as it applies to code this fork owns (see D5).
- Breaking API changes are allowed where the go-live plan calls them correct (R-P0-2/3/4/9);
  each one gets a line in `CHANGELOG.md` and in `KRX_FORK_NOTES.md` for the gateway.

## 3. Decisions

| # | Decision | Default (used unless the owner says otherwise) | Needed by |
|---|---|---|---|
| D1 | Threading default (R-P0-8, pending in `KRX_FORK_NOTES.md`) | **Decided by the owner, 12 Nov 2026:** `MultiThreaded` becomes the default; `SingleThreaded` stays, marked `#[deprecated]` | phase 6 |
| D2 | Version after the breaking changes | `0.3.0` on `krx/main` at the end of phase 8; gateway moves in one PR. **Done on top of 10b**, every workspace crate at 0.3.0 | phase 8 |
| D3 | Sanitizer toolchain in CI | nightly `-Zsanitizer`, clang 18 for the C++ side, Linux x86_64. ASan without `-Zbuild-std` (verified locally on aarch64 Linux). TSan with `-Zbuild-std`, because an uninstrumented std makes TSan report libtest's own channels as races; `-Zbuild-std` crashes every libtest binary on aarch64 and TSan cannot run under x86_64 emulation, so the TSan job is first verified by CI | phase 1 |
| D4 | Red tests before their fix | under a sanitizer: listed in `scripts/san-known-red.txt` and skipped by `scripts/san.sh`; in plain `cargo test`: `#[ignore = "red until <item>: why"]`, and `cargo test -- --ignored` shows them red. Each fix phase removes its entries; none left by phase 9 | phase 1 |
| D5 | Lint scope | **Decided by the owner, 12 Nov 2026.** New files must pass `lint.sh`. `quickfix-ffi/quickfix-bind` and `libquickfix` count as vendored upstream code: changed lines follow the house style where they can, the files are not reformatted, and the hook's findings on them are reported in each handover. Whether to exempt vendored C++ in the central policy is the owner's later call | every phase |
| D6 | TSan gate | **Decided by the owner, 12 Nov 2026.** The first CI TSan run found a race in QuickFIX's own `FIX::Mutex::lock()` (unsynchronised read of the owner in its recursive lock; `Initiator::isLoggedOn` vs `connect`). Until a C++ pin fixes it, the TSan job is report-only (`continue-on-error`, `halt_on_error=0`) and lists every distinct race in its job summary; C++ races go to the C++ fork, Rust-side races are fixed here. ASan stays blocking. TSan becomes blocking again after the bump | phase 1 |
| D7 | `cargo deny` (R-P1-6) | **Decided by the owner, 12 Nov 2026.** No repo `deny.toml` (policy). CI here runs `cargo audit` (advisories, no config); licenses, bans and duplicates are checked by `cargo deny` with the central config on the gateway's own `Cargo.lock`, which is what ships | phase 10 |
| D8 | `clippy -D warnings` (R-P1-6) | **Decided by the owner, 12 Nov 2026.** Fix the warnings in upstream tests and examples, deny warnings on the whole workspace, on a pinned toolchain (1.99.1) bumped on purpose | phase 10 |
| D9 | FFI naming lint | **Decided by the owner, 12 Nov 2026.** Keep `#![allow(non_camel_case_types, non_snake_case)]` in `quickfix-ffi` (names mirror the C ABI); the owner adds a narrow exemption for FFI crate roots to the central policy | phase 10 |
| D10 | Phase 10 size | R-P2-2 is eight changes, several new API; split into 10a (build, CI, docs: R-P1-5, R-P1-6, R-P1-7, R-P2-7) and 10b (R-P2-2) | phase 10 |
| D11 | C++ fork work (R-P0-12, R-P0-13, races) | **Decided by the owner, 12 Nov 2026:** not now; the 11 R-P0-13 tests stay red and TSan stays report-only until then | — |
| D12 | Production distro | **Decided by the owner, 12 Nov 2026:** RHEL/Rocky Linux 9; CI gets a Rocky 9 job (phase 13) | phase 13 |
| D13 | Fuzzing | **Decided by the owner, 12 Nov 2026:** a separate `fuzz/` crate outside the workspace (`libfuzzer-sys` allowed) with a short nightly run (phase 14) | phase 14 |
| D14 | Gateway work | **Decided by the owner, 12 Nov 2026:** done by the gateway team from `KRX_GATEWAY_HANDOVER.md`, not in this repo | — |
| D15 | msgkrx licence; SharePoint link | **Decided by the owner, 12 Nov 2026:** licence stays `MIT OR Apache-1.1`. The repo is a public fork: history is not rewritten; the owner revokes the old SharePoint link | — |
| D16 | Upstream (R-P2-4, R-P2-6) | **Decided by the owner, 12 Nov 2026:** this stays our fork only: no fast-forward of `main`, nothing sent upstream | — |
| D17 | Taking the C++ fork's fixes (supersedes D11 for what is fixed there) | **Decided by the owner, 13 Nov 2026:** the C++ fork's own V1 plan (`gianghn/quickfix`, `docs/V1_PLAN.md`) fixed P0-1 (= R-P0-13), the `FIX::Mutex`/`SessionState` races (#772–#781), the plain acceptor's triple close (K-6) and send truth (P0-6, = R-P0-12 in C++); phase 16 pins `krx/main` @ `11703b2a`. BDD rows here are `cargo test` tests named after the behaviour (the C++ fork's feature-files plan: Gherkin only in the gateway) | phase 16 |
| D18 | TSan gate after the bump (changes D6) | **Decided by the owner, 13 Nov 2026:** TSan becomes blocking (phase 18) once the pinned C++ leaves no reports, or only ones its phase 6 owns | phase 18 |

## 4. Phases

Risk class: **H** = memory/thread soundness or message loss, **M** = API or wire behaviour,
**L** = build, docs, tooling.

| Phase | Items | Risk | Red first | Green when |
|---|---|---|---|---|
| 1 Sanitizer CI | R-P1-6 (part): ASan + TSan jobs, 20× socket-test loop, submodule pin check | L | ASan run of today's suite: record which tests report | jobs green with the known-red list (D4) |
| 2 KRX test suite | new `quickfix-msgkrx/tests/` with KRX 35=D / 35=8 built from FIX44KRX.xml | M | tests that read groups report under ASan; generated-getter tests for 43=Y, 150=C, missing tags panic | characterisation tests green; red ones added to known-red with their item |
| 3 Callbacks fail closed | R-P0-1 panic → abort; R-P0-5 -6 → `UnsupportedMessageType`, fail-closed `default:`; R-P2-1 `catch (...)` | H | death test: panic in `from_app` keeps the session up today; -6 test: peer gets no 35=j | child aborts on every callback panic; peer gets 35=j 380=3 |
| 4 Group copy | R-P0-7 `clone_group` over-read, `getGroupRef`, generator borrows groups | H | `test_message_group` under ASan | ASan clean; known-red entries removed |
| 5 Ownership and threads in types | R-P0-2 `from_fix_message_t` → `unsafe` / `into_raw`; R-P0-4 `Send + Sync` on callback traits | H | `compile_fail` doctests compile today | doctests fail to compile; ASan round-trip test clean |
| 6 Lifecycle | R-P0-8 tests on MultiThreaded + D1; R-P0-6 drop/send gate | H | 50× loop shows `stop()` crashes; ASan drop-while-sending test reports | loop 50/50; ASan clean over 50 seeded runs |
| 7 Borrowed views | R-P0-3 `MessageMut<'a>`, `HeaderMut<'a>`, `TrailerMut<'a>`, `GroupMut<'a>` | H | `mem::swap` on a callback header compiles today | swap is a `compile_fail` doctest; callback suite ASan clean |
| 8 Generator | R-P0-9 `try_get_*`, `TryFrom<Message>`, Y/N bool; R-P0-10 reject NaN/inf | M | phase 2 red tests; `44=NaN` accepted | phase 2 suite fully green; property test on ticks |
| 9 Binding errors and bytes | R-P1-1 seqnum APIs; R-P1-2 stale last error; R-P1-3 log bytes; R-P1-4 `memcpy` | M | `set(0)` accepted; stale error test; `58=caf\xE9` and `96=a\0b` differ from the wire | each test green; known-red list empty |
| 10 Build and hygiene | R-P1-5 hermetic hardened build; R-P1-6 rest (release `--locked`, MSRV, deny/audit, SHA-pinned actions); R-P2-2 small fixes; R-P1-7 reset docs; R-P2-7 docs/metadata | L | no-submodule build tries the network; `checksec` lacks flags | CI fails cleanly offline; flags present; each R-P2-2 unit test green |
| 11 Regression harness | R-P0-13 data-length test (95 = 1e5 / 1e8 / 2e9 / -50, raw TCP and in process, acceptor and initiator); the research harness's panic, re-entry, cross-session and raw-peer modes and its FFI repro programs as tests in this repo | H | 95=1e8 kills the process under ASan (red, `#[ignore]` / known-red until the C++ pin); repros not yet in the repo | every repro a test; R-P0-13 tests listed red with their item; the rest green |
| 12 Build leftovers and lint | R-P1-5 rest: identical `libquickfix.a` from two clean builds, split debuginfo; the ~60 fixable policy-clippy findings on go-live lines (handover 10b) | L | two clean builds differ / no debuginfo split; findings counted | identical hashes; split debuginfo; those findings 0 |
| 13 Rocky Linux 9 | D12: CI job in a pinned `rockylinux/rockylinux:9` container: tests, hardening and reproducibility checks | L | `ar` not deterministic on Rocky without phase 12's flags | job green |
| 14 Fuzzing | D13: `fuzz/` crate outside the workspace, C++ instrumented, nightly workflow | M | the old fuzzer never finds the R-P0-13 over-read | the new one finds it; nightly green with the known crash filtered |
| 15 Byte-level parse API | `Message::try_from_bytes`, `DataDictionary::try_build_message_bytes` over `FixMessage_fromBytes(ptr, len)`; fuzz targets switch to it (handover 14, decision 1) | M | non-UTF-8 and embedded-NUL frames cannot be parsed from Rust | they parse byte-exact; fuzz targets drop the UTF-8 skip |
| 16 C++ fixes pinned | D17: `libquickfix` → C++ fork `krx/main` @ `11703b2a`; R-P0-13 tests un-ignored, known-red list empty, fuzz `skip-r-p0-13` filter gone (its crashes become seeds); TSan counted on the new pin | H | on `33b12daf` 17 of 20 data-length rows red under ASan (checklist below) | all 20 green under ASan; known-red empty; fuzz finds nothing with the filter gone |
| 17 Send truth to Rust | R-P0-12: `FixSession_sendToTarget` keeps C++'s `bool`; `send_to_target` errors when the message is stored but not sent | M | a send while disconnected returns `Ok` | it returns the not-sent error |
| 18 TSan blocking | D18: remove `continue-on-error` from the TSan job | L | TSan job red but ignored | job blocking and green |

Phase 16 BDD checklist (`quickfix/tests/test_data_length.rs`; red = on `33b12daf` under ASan):

| Row | Behaviour | Tests | Red on old pin |
|---|---|---|---|
| 16.1 | An acceptor refuses a Logon whose 95 is 1e5, 1e8, 2e9 or -50 and keeps serving | `acceptor_refuses_logon_when_95_is_*` (4) | 4 (3 heap over-reads, 1 `std::length_error` abort) |
| 16.2 | An initiator refuses such a Logon reply and stays logged off | `initiator_refuses_logon_reply_when_95_is_*` (4) | 4 (same) |
| 16.3 | A dictionary refuses such a frame, text or bytes | `try_build_message{,_bytes}_returns_error_when_95_is_*` (8) | 6 (the two -50 rows were already green in process) |
| 16.4 | SecureData (90/91), the field the fuzzer crashed on, is refused like 95/96 | `try_build_message_bytes_returns_error_when_90_is_1e5` | 1 |
| 16.5 | Without a dictionary no field is a data field: 91 and 96 end at the next SOH | `try_from_bytes_ends_{91,96}_at_soh_*` (2) | 2 by expectation (written as "refused"; the old and new pin both return `Ok`, never over-read; corrected) |
| 16.6 | A zero-length data field still parses (guard for the bounds check) | `try_build_message_bytes_parses_a_zero_length_data_field` | 0 (guard) |
| 16.7 | The four former fuzz crashes are refused by a dictionary and do not crash without one | `fuzz`: `former_r_p0_13_crashes_are_refused_by_the_dictionary` | — (seeds replayed by every fuzz run) |

Order reasoning: sanitizers first, because most P0s are only visible under them; the KRX suite
next, so every later phase has KRX-shaped messages to test with; then the crashers and silent
loss (3, 4) before the API redesigns (5, 7, 8) that the gateway must adapt to.

## 5. Progress

| Phase | Branch | Status | Red → green |
|---|---|---|---|
| 1 | `krx/golive-p1` | PR #2 | ASan: 6 overflow tests + 11 leaking tests → leaks fixed, 6 on known-red (R-P0-7); TSan unverified until CI |
| 2 | `krx/golive-p2` | PR #3 | 8 tests: 6 green, 1 red under ASan (R-P0-7), 1 red `#[ignore]` (R-P0-9) |
| 3 | `krx/golive-p3` | PR #4 | 4 red (10/10 panics survived; from_app panic consumed; no 35=j for -6; SSL stub killed the process) → all green |
| 4 | `krx/golive-p4` | PR #5 | 6 ASan overflows + 2 new tests red (field id 3074920) → known-red list empty, ASan exit 0; `with_group` ref part red `#[ignore]` until phase 7 |
| 5 | `krx/golive-p5` | PR #7 | 3 `compile_fail` doctests compiled + round trip did not build → all green, ASan exit 0 |
| 6 | `krx/golive-p6` | PR #8 | default was SingleThreaded; drop-while-sending UAF 1/3 under ASan → 0/10 |
| 7 | `krx/golive-p7` | PR #9 | 4 `compile_fail` doctests compiled → do not; `with_group` field id test enabled |
| 8 | `krx/golive-p8` | PR #10 | `43=Y` → `None`, NaN accepted, `try_get`/`TryFrom` missing → all green, KRX 13/13 |
| 9 | `krx/golive-p9` | PR #11 | stale error code -15, set(0) accepted, log/`to_fix_bytes` lost bytes after NUL → all green, ASan exit 0 |
| 10a | `krx/golive-p10` | PR #12 | hardening check 7 failures (git called, 4 flags missing, `-w`, shared cache) → 0, macOS and Linux |
| 1–10b | — | merged into `krx/main` (`4dcb99f`), 12 Nov 2026 | TSan report-only: all 170 reports C++-only (D6) |
| 10b | `krx/golive-p10b` | PR #13; version 0.3.0 (D2) | 13 compile errors on the new API, `get::<i32>` of a missing key `Ok`, drop 15.0 s, safe store impl compiled → all green; drop 4.7 s |
| 11 | `krx/golive-p11-harness` | PR #16, merged | R-P0-13: 11 of 12 data-length tests red under ASan (heap-buffer-overflow in `Message::extractField`, or `std::length_error` → terminate for 95=-50), `#[ignore]` + known-red until the C++ pin; re-entrant and cross-session sends green; opt-in soak green (1.49 M messages, 0 lost) |
| 12 | `krx/golive-p12` | PR #15, merged | macOS archives differ (Linux already identical), release binary has no debug info to split, policy clippy on go-live lines 90 → identical on both, 1.47 MB binary + 32.7 MB `.debug`, 30 (29 `unsafe_code`, 1 kept `unnecessary_wraps`) |
| 13 | `krx/golive-p13` | PR #18, merged | Rocky `ar` without `D` → archives differ; with phase 12's flags identical; tests 150/0/13 |
| 14 | `krx/golive-p14` | PR #19, merged | old fuzzer 0 crashes in 65.4 M inputs (88 edges) → C++ instrumented: over-read in 1.27 M inputs, ~105 s (1 099 edges) |
| 15 | `krx/golive-p15-api` | PR #20, merged | tests do not compile without the API → macOS 158/0/16, ASan 158/0/2; fuzz 4 × 60 s via bytes, 0 crashes |
| 16 | `krx/golive-p16` | PR open | 17 of 20 data-length rows red under ASan on `33b12daf` → 20/20 on `11703b2a`; macOS 176/0/2, ASan 176/0/2 with no known-red; fuzz 4 × 60 s without the filter, 0 crashes |

## 6. Handover notes

### Handover 1 — 2026-11-12
**Built:** R-P1-6 (part): `scripts/san.sh` (ASan/TSan for C++ and Rust), `scripts/san-known-red.txt`,
`scripts/test-loop.sh` (socket tests N times), `scripts/check-submodule-pin.sh`; CI jobs `sanitizers`
(address, thread), `socket-loop` (20 runs), `submodule-pin`. R-P1-2 (leak part only, moved here
because leak reports cannot be skipped per test): the binding's last-error buffer is a
`thread_local std::string` instead of a raw `char*` that leaked once per thread.
**Red → green:** first ASan run of the existing suite (Linux aarch64 container): 4 binaries failed.
Heap-buffer-overflow in `FixMessage_copyGroup` / `FixHeader_copyGroup` (`test_read_group_clone`,
`test_build_custom_with_recursive_groups`, msg40 `test_build_list_status`, `test_group_iterator`;
R-P0-7) and LeakSanitizer in 11 tests of `test_dictionary`, `test_field_map`, `test_messages`
(error buffer, R-P1-2). After: leaks gone; 99 tests ASan-clean run one by one; the 5 R-P0-7 tests
are on the known-red list; `scripts/san.sh address` exits 0.
**Checks:** `cargo fmt --check` ok; `cargo test --workspace` (macOS) all green; `cargo clippy`
unchanged (upstream test warnings only); shellcheck clean; `check-submodule-pin.sh` ok (33b12daf
on krx/main); `test-loop.sh 10` on Linux: 10/10 green (the review saw 6/10 fail under load; CI runs 20).
**Not verified:** the TSan job. TSan needs `-Zbuild-std` (otherwise it flags libtest's own
channels, seen here), `-Zbuild-std` crashes every test binary on aarch64, and TSan cannot run under
x86_64 emulation. Its first real run is on GitHub's x86_64 runner; expect findings to triage there.
**Decisions taken:** D3 (toolchain, revised twice by evidence), D4 (known-red list + `#[ignore]`),
D5 decided by the owner (vendored C++ is not reformatted).
**Away from the plan:** R-P1-2's leak fix moved from phase 9 to phase 1 (the rest of R-P1-2 stays
in phase 9).
**Left for the owner:** the local submodule checkout is dirty from the C++ lint hook
(`src/C++/config.h` modified, absolute symlinks into `libquickfix/build/cpp-policy`); builds in a
clean checkout ignore it, but `git -C quickfix-ffi/libquickfix checkout -- . && clean -fdx` would
reset it. Push and PR wait for the owner.

### Handover 2 — 2026-11-12
**Built:** `quickfix-msgkrx/tests/test_krx_messages.rs`, the crate's first tests. Messages are framed
by a `wire()` helper (correct 9= and 10=) with the KRX header (128/129 required) and parsed through
`DataDictionary::try_from_path(FIX44KRX.xml)`: NewOrderSingle (35=D) and a Quote (35=S) with two
parties, the second with two nested sub-IDs.
**Red → green:** 8 tests. Green: dictionary loads; every field survives `to_fix_string` (QuickFIX
reorders by tag, so the first byte-for-byte version of that test was wrong and was rewritten);
raw header/body `get_field`; generated NewOrderSingle getters; raw `with_group` parties; 43 absent →
`None`. Red under ASan: `iter_no_party_i_ds_reads_parties_and_nested_sub_ids`, heap-buffer-overflow
in `FixGroup_copyGroup` on the nested group (R-P0-7, known-red). Red in plain tests:
`get_poss_dup_flag_returns_true_when_43_is_y` returns `None` for `43=Y` (R-P0-9, `#[ignore]`).
**Checks:** native `cargo test -p quickfix-msgkrx`: 7 passed, 1 ignored; `--ignored` fails as
expected; `scripts/san.sh address` (whole workspace): exit 0; `cargo fmt --check` ok.
**Not covered yet:** ExecutionReport (35=8) with `150=C`, `58`, and missing required tags: today's
getters panic and the dictionary rejects them (R-P0-11). Those tests are written in phase 8
against the `try_get_*` API, and R-P0-11's part waits for the member spec.
**Decisions taken:** none new.

### Handover 3 — 2026-11-12
**Built:** R-P0-1: `utils::abort_on_panic` wraps every application and log callback; a panic
aborts the process. R-P0-5: `fromApp` maps -6 to `UnsupportedMessageType`; `toApp`, `fromAdmin`
and `fromApp` abort on a result code they do not know (fail closed). R-P2-1: `catch (...)` in
every binding wrapper (`Fix_setLastErrorText`), SSL stub throws `std::logic_error` by value.
The callback traits lost upstream's `#[allow(unused_variables)]` (unused default-method
parameters are `_`-prefixed instead). Docs: `doc/DEV_NOTES.md`, `quickfix/CHANGELOG.md`,
`KRX_FORK_NOTES.md`.
**Red → green:** `quickfix/tests/test_callback_failures.rs` and `quickfix-ffi/tests/test_ssl_stub.rs`.
Red: a panic in each of the 10 callbacks (create, logon, logout, to/from admin, to/from app, log
incoming/outgoing/event) left the child process running (0/10 aborts); a `from_app` panic
consumed the message; `UnsupportedMessageType` reached the peer as nothing; the SSL stub killed
the test process ("Rust cannot catch foreign exceptions"). Green: 10/10 SIGABRT; the receiver's
FileStore still expects 34=2 after the abort; the peer gets 35=j 380=3; both SSL constructors
return `None` with `ERRNO_EXCEPTION`. FieldNotFound (35=j 380=5), IncorrectDataFormat (35=3
373=6) and IncorrectTagValue (35=3 373=5) were already right and stay as characterisation tests.
**Checks:** `cargo test --workspace` (macOS) all green; `scripts/san.sh address` (Linux) exit 0;
`cargo fmt --check` ok; clippy: no warning in changed files (upstream test warnings unchanged).
**Not covered:** the go-live plan's "the peer's resend delivers the message after restart" part of
the R-P0-1 test: the seqnum check proves the message is not consumed; a restart-and-resend test
needs the phase 6 lifecycle work and is added there.
**Decisions taken:** unknown callback codes abort rather than map to a reject (the Rust side
cannot produce them, so one means the two sides disagree).
**Hook findings (D5):** `quickfix_bind.cpp` still reports the pre-existing whole-file formatting,
`new`/`delete` and `strncpy` (R-P1-4, phase 9) findings; the changed lines follow the house style.

### Handover 4 — 2026-11-12
**Built:** R-P0-7 copy part. QuickFIX stores every group entry as a plain `FieldMap`; the four
`*_copyGroup` wrappers cast it to `Group` and copy-constructed a `Group`, reading `m_field` and
`m_delim` past the object. One helper, `copyGroupEntry`, now builds a real `Group(tag, delim)`
(delim = the entry's first field) and assigns the `FieldMap` into it (`std::make_unique`, so the
four naked `new`s are gone too).
**Red → green:** the 5 known-red ASan tests (`test_read_group_clone`,
`test_build_custom_with_recursive_groups`, msg40 `test_build_list_status`, `test_group_iterator`,
KRX `iter_no_party_i_ds_reads_parties_and_nested_sub_ids`) plus two new plain tests
(`clone_group_returns_group_with_its_field_id_and_delim`,
`clone_group_of_header_and_nested_group_returns_groups_with_their_ids`: message, header and
nested group) that read field id 3074920 / 3075360 instead of 33 / 627. After: all green, the
known-red list is empty, `scripts/san.sh address` exit 0 on the whole workspace.
**Checks:** native `cargo test` on quickfix, msg40, msgkrx green; ASan (Linux) exit 0; fmt ok.
**Away from the plan:** the `getGroupRef` half of R-P0-7 moves to phase 7. `with_group` /
`with_group_mut` hand the stored entry (a `FieldMap`) out as a `&Group`; `field_id()`, `delim()`
and `Debug` on it read garbage (`with_group_reports_field_id_and_delim_of_the_entry`, red,
`#[ignore]`). The fix is phase 7's borrowed view type, which has no field id. For the same reason
the generator keeps `clone_group` (now correct) instead of switching to `with_group`.
**Decisions taken:** delimiter of a copied entry = its first field, which is what QuickFIX's own
ordering puts first.

### Handover 5 — 2026-11-12
**Built:** R-P0-4: `ApplicationCallback: Send + Sync`, `LogCallback: Send + Sync`. R-P0-2:
`Message::from_fix_message_t` is `unsafe fn` with a `# Safety` contract; new
`Message::into_fix_message_t(self)` releases ownership for it. The test `FixRecorder` keeps the
expected session as its repr string (a `SessionId` is a raw pointer, so not `Send + Sync`).
**Red → green:** 3 `compile_fail` doctests (`from_fix_message_t` without `unsafe`; an
application and a logger holding a `Cell`) compiled before, and fail to compile now; the ASan
round trip `into_fix_message_t_then_from_fix_message_t_round_trips_ownership` did not compile
(no `into_fix_message_t`) and is green and ASan-clean now.
**Checks:** `cargo test --workspace` (macOS) green, doctests 4/4; `scripts/san.sh address`
(Linux) exit 0; fmt ok.
**Breaking for the gateway:** its application and log types must be `Send + Sync` (the gateway's
own Phase 0 compile-time assertion asks the same); every `from_fix_message_t` call needs an
`unsafe` block that states where the pointer came from.
**Not done:** the go-live plan's "TSan run with one app shared by two MultiThreaded sessions":
TSan is report-only (D6) and the C++ races below drown it; it comes back with the C++ pin bump.

#### TSan inventory, first full CI run (PR #5, 114 reports, report-only per D6)
All in the C++ library; none on the Rust side. For the C++ fork:
| Location | What races | Seen in |
|---|---|---|
| `Mutex.h:62/63/78` `FIX::Mutex::lock/unlock` | recursive lock reads its owner without synchronisation | every session test (62 reports) |
| `SocketAcceptor.cpp:130` `onStart` / `SocketServer::~SocketServer` vs `close()` | plain acceptor freed while `stop()` closes it (R-P2-3) | SingleThreaded tests |
| `SessionState.h:59-68` `receivedLogon`, `sentLogon`, `enabled`, `lastSentTime` | logon flags and timestamps written by `send`/logon on one thread, read by `next`/`isLoggedOn` on another | MultiThreaded too (`test_callback_failures`) |

### Handover 6 — 2026-11-12
**Built:** R-P0-6: a process-wide `SESSION_GATE` (`session.rs`). `send_to_target` and
`Session::send` hold it shared (`while_sending`; a send nested in a callback on the same thread
skips it); `Acceptor`/`Initiator` `Drop` calls `stop()` first, then deletes under the exclusive
gate (`while_deleting`). Taking the gate before `stop()` would deadlock: stop joins QuickFIX's
threads, whose callbacks may send. R-P0-8 / D1: `MultiThreaded` is the default and
`SingleThreaded` is `#[deprecated]`; tests, examples, README and doc example use `MultiThreaded`;
the `SingleThreaded` send/receive test is gone; the checkers wait for **both** sides to log on
(with a 10 s bound) instead of either.
Also the R-P0-1 follow-up from phase 3: `from_app_panic_message_is_delivered_after_restart`
(both sides on FileStores; after the child aborts, a restarted pair delivers the message).
**Red → green:** `fix_socket_server_kind_default_is_multi_threaded` was `SingleThreaded`.
`dropping_initiator_and_acceptor_while_another_thread_sends_is_safe` (10 drop/recreate cycles
while a thread sends): ASan heap-use-after-free in `MemoryStore::set` in 1 of 3 runs before; 0 in
10 runs after. The restart test was green on first run: the phase 3 fix was already in, so it is
a regression test, not a red-first one.
**Checks:** `cargo test --workspace` (macOS) green, no warnings; `scripts/san.sh address` exit 0;
callback tests under ASan 8/8; fmt ok; clippy: nothing in changed files.
**Away from the plan:** `SslSingleThreaded` is documented as "avoid", not `#[deprecated]`: the
lint fires inside this crate too, and the crate must name one single-threaded kind to tell the
two apart; `#[allow(deprecated)]` is not allowed (policy). The go-live plan asks for 50 seeded
runs of the drop test; 10 ran here, CI runs it on every push.
**Decisions taken:** none new (D1 by the owner).

### Handover 7 — 2026-11-12
**Built:** R-P0-3: new `view.rs`. `Borrowed<'a, T>` (aliases `MessageMut`, `HeaderMut`,
`TrailerMut`) wraps a C++ object owned elsewhere: no `Drop`, `Default` or `Clone`, reads through
`Deref` to `&T`, writes through `FieldMap`, no `&mut T`. `GroupMut<'a>` is the group-entry view:
it reports the tag it was looked up by and has no delimiter, because QuickFIX stores entries as
plain field maps (the deferred half of R-P0-7). `with_header_mut`, `with_trailer_mut`,
`with_group`, `with_group_mut` and the `on_msg_to_admin` / `on_msg_to_app` callbacks hand out
these views. Nothing a caller can move out of a view frees anything; two views cannot be swapped
across borrows (`&mut` is invariant in the view's lifetime), and swapping two views of the same
borrow frees nothing either.
**Red → green:** 4 `compile_fail` doctests (swap a borrowed header out, `mem::take` a trailer,
swap a group entry, `mem::take` the message in `on_msg_to_app`) compiled before and do not now.
`with_group_reports_field_id_of_the_entry` (was the ignored phase 4 test, now checking field id
and a field instead of the delimiter an entry does not have) is enabled and green.
**Checks:** `cargo test --workspace` (macOS) green, doctests 8/8; generated message crates build
unchanged (their closures infer the new types); `scripts/san.sh address` exit 0; fmt ok.
**Breaking for the gateway:** `on_msg_to_admin` / `on_msg_to_app` take `&mut MessageMut<'_>`;
`with_*_mut` closures get `&mut HeaderMut<'_>` / `&mut TrailerMut<'_>` / `&mut GroupMut<'_>`;
`with_group` gives `&GroupMut<'_>`. Closures that let the type be inferred keep compiling; named
`fn(&mut Header)` helpers must change their parameter type. Reads still work through `Deref`.
**Away from the plan:** `GroupMut` still calls the `FixGroup_*` field functions on the stored
`FieldMap` pointer: they only touch the `FieldMap` base, so ASan is clean, but strictly it is a
`Group*` that points to a `FieldMap`. A `FixFieldMap_*` C API would remove that; not done here.
**Decisions taken:** one generic `Borrowed<T>` instead of four hand-written view types.

### Handover 8 — 2026-11-12
**Built:** R-P0-9 in the generator. Each crate gets `FieldError { Missing(tag), Invalid(tag) }`
and `FromFixStr` (booleans parse as FIX `Y`/`N`; enums through `from_const_bytes`; numbers and
strings through `FromStr`). Every field gets `try_get_*`: required fields return
`Result<T, FieldError>`, optional ones `Result<Option<T>, FieldError>`, so a present but unknown
value (`150=C`) is `Invalid`, not a silent `None`. The old getters stay, built on `try_get_*`
(required ones still panic, now documented with `# Panics`). `TryFrom<quickfix::Message>`
(error `FieldError`, `Missing(35)` / `Invalid(35)`) replaces the asserting `From`; the message
cracker builds the structs directly. R-P0-10: `IntoFixValue` for `f32`/`f64` rejects NaN and
infinities; the trait's error type is `QuickFixError` (was `NulError`) so it can say why.
**Red → green:** KRX suite: `get_poss_dup_flag_returns_true_when_43_is_y` (ignored since phase 2,
`None` for `43=Y`) enabled and green; new `try_get_exec_type_returns_value_when_150_is_known`,
`..._invalid_when_150_is_unknown`, `try_get_exec_id_returns_missing_when_17_is_absent`,
`try_get_optional_field_tells_absent_from_invalid`,
`try_from_message_returns_error_when_msg_type_differs` did not compile before (no API) and are
green; `set_field_returns_error_when_float_is_not_finite` accepted NaN before, rejects it now.
KRX suite 13/13, nothing ignored.
**Checks:** `cargo test --workspace` (macOS) green, no warnings; all generated crates (FIX 4.0 to
5.0, KRX) build; `scripts/san.sh address` exit 0; fmt ok.
**Not done:** a decimal price type in the generator (the go-live plan's "optionally"); prices stay
`f64` aliases. The tick property test was dropped: the house rule bans `f64` money even in tests,
and the gateway writes prices as decimal strings (Phase 0). Unknown-enum values are only readable
as raw strings (`get_field`); adding an `Unknown(String)` variant to every enum would be a larger
API change.
**Breaking for the gateway:** `From<Message>` for generated messages is gone, use `try_from`;
custom `IntoFixValue` impls return `QuickFixError`.

### Handover 9 — 2026-11-12
**Built:** R-P1-1: `set_next_*_msg_seq_num(u64)` rejects 0 and values above `i32::MAX`;
`get_expected_*_num() -> Result<u64, _>` (negative = the binding's error code, no longer a
seqnum); docs say to call the setters while logged out (QuickFIX takes no session lock there).
R-P1-2 (rest): every binding wrapper resets the error state on entry (`Fix_resetLastError`), and
`Session::lookup` reports `SessionNotFound` itself instead of the thread's last error. R-P1-3:
the log callback ABI passes `(ptr, len)`; `LogCallback` gains `on_*_bytes` (default: lossy text
to the old `&str` methods, so existing loggers compile); new `Message::get_field_bytes` and
`FixMessage_getFieldBytes`. R-P1-4: both `readString` copies use `std::copy` of every byte.
**Red → green:** `test_seqnum.rs` (4 tests: set 0 / above `i32::MAX` rejected, get returns what
was set, lookup after an unrelated error is `SessionNotFound`) did not compile on the old API;
`quickfix-ffi/tests/test_last_error.rs`: a successful call left code -15 → 0;
`test_wire_bytes.rs` (raw TCP peer sends `58=caf\xE9` and `95=3|96=a\0b`): with the log ABI and
`get_field_bytes` in, the only failure left was "to_fix_bytes zero-filled after the NUL"
(R-P1-4); green after `std::copy`.
**Checks:** `cargo test --workspace` (macOS) green, no warnings; doctests 8/8;
`scripts/san.sh address` exit 0; fmt ok.
**Breaking:** `set_next_*`/`get_expected_*` signatures (u64, `Result`); the C log callbacks take
a length (C and Rust FFI users).
**Raised with the owner:** the hook rejects upstream's `#![allow(non_camel_case_types,
non_snake_case)]` at the top of `quickfix-ffi/src/lib.rs`, which exists because the FFI names
mirror the C ABI (`FixMessage_t`, `onIncoming`). Removing it means renaming the whole C binding;
not done.

### Handover 10a — 2026-11-12
**Built:** R-P1-5: `build.rs` no longer runs `git submodule update`; a missing submodule stops
the build with the command to run. sccache/ccache only with `QUICKFIX_FFI_COMPILER_LAUNCHER`.
`build.rs` sets `CMAKE_C/CXX_FLAGS` itself (cmake-rs adds `-w`, which hid every warning):
`-Wall -fno-omit-frame-pointer -fstack-protector-strong -ffile-prefix-map`, plus
`-fstack-clash-protection` and `-D_FORTIFY_SOURCE=2` on Linux (FORTIFY left out under a
sanitizer); the user's `CFLAGS`/`CXXFLAGS` are kept, so `san.sh` still instruments the C++.
libquickfix builds in its own directory (`libquickfix-out`). MSVC keeps cmake-rs's flags.
R-P1-6: `krx_ci.yml` gains `msrv` (1.86, `--locked`), `release` (`--release --locked`),
`build-hardening`, `audit` (D7), `ssl` (`build-with-ssl`); clippy on 1.99.1 with `-D warnings`
(D8) after fixing 18 warnings in upstream tests/examples; every action pinned by SHA;
`permissions: contents: read`. R-P1-7: `Session::reset` rustdoc warns it wipes the store;
`doc/RUNBOOK_SEQNUM.md`. R-P2-7: README no longer says "ready for production"; msgkrx crate
doc names FIX44KRX.xml; msgkrx authors; the SharePoint share link in FIX44KRX.xml replaced by
the workbook version. `.gitignore` covers the local C++ lint build dirs.
**Red → green:** `scripts/check-build-hardening.sh`: 7 failures (git run for the missing
submodule, -Wall / frame pointers / stack protector / prefix map missing, `-w` present, one
CMake cache shared by both builds) → 0 on macOS and Linux (FORTIFY and stack-clash checked).
**Checks:** `cargo test --workspace` green (macOS); doctests 8/8; clippy `-D warnings` exit 0;
`cargo +1.86.0 check --workspace --all-targets --locked` ok; `cargo audit` 0 advisories; SSL
feature tests green (Linux container); `scripts/san.sh address` exit 0 (Linux); fmt ok.
`build.rs` returns a typed `BuildError` instead of `expect`/`panic!` and is clean under the
policy's clippy. **Corrected in 10b:** this note first said every Rust file the phase changed was
clean; the policy's clippy never reached the `quickfix` crate (it stops at `quickfix-spec-parser`
and `quickfix-ffi`, and `--all-features` needs MySQL), so only `build.rs` was checked. See
handover 10b for the real numbers.
**Found on the way:** `fix_repl`'s `parse_send_to` consumed the `send_to` token inside a
`debug_assert_eq!`, so release builds read the command word as the message; fixed.
**Away from the plan:** not done here: identical `libquickfix.a` from two clean builds, split
debuginfo, a production-matching distro image (which distro does the gateway ship on?), and the
30-run reset chaos test (R-P1-7 evidence, needs the C++ pair).
**Left for the owner:** the SharePoint link is still in git history; msgkrx's `license` is
upstream's `MIT OR Apache-1.1` (the QuickFIX licence file) — is that right for the KRX
dictionary? D9 exemption in the central policy.

### Handover 10b — 2026-11-12
**Built (R-P2-2):** `FfiMessageStoreFactory` is an `unsafe trait` (SAFETY on each impl);
`Dictionary::get::<i32|f64>` returns the binding's error instead of `Ok(0)` (out-parameter ABI);
`FixSession_send` null-checks `msg`; rejects carry their tag and logon rejects their text
(`MsgFromAppError`/`MsgFromAdminError` variants take `{ tag }` / `{ text }`, handed to C++ through
the thread-local `FixApplication_setRejectDetail`); `ConnectionHandler::stop_force`, used by
`Drop`; `Session::is_enabled`, `refresh`, `send_test_request`; `SocketSendBufferSize` /
`SocketReceiveBufferSize` are `u32`, refused above `i32::MAX`; typed `TimestampPrecision`.
Raw-peer helpers moved to `tests/utils/raw_peer.rs`.
**Red → green:** `test_session_controls.rs` (5 tests) and the reject tests in
`test_callback_failures.rs` did not compile on the old API (13 errors); `get::<i32>("missing")`
was `Ok`; dropping an acceptor whose peer never answers the Logout took 15.0 s; a safe
`impl FfiMessageStoreFactory` compiled. Now: Session rejects carry RefTagID 371=148 / 33; the
business reject names tag 1 in its text; the logon reject's Logout says `Rejected Logon Attempt:
outside trading hours`; TestRequest `PING-1` comes back as a Heartbeat with 112=PING-1; drop takes
4.7 s; the doctest fails to compile. Upstream `test_dictionary` expected `Ok(0)` for a missing
key and for NaN/inf; it now expects errors.
**Checks:** `cargo test --workspace` green (macOS); doctests 9/9; clippy `-D warnings` exit 0;
`scripts/san.sh address` exit 0 (Linux); fmt ok.
**Away from the plan:** no Rust test for the `FixSession_send` null check (`FixMessage_t` is
`NonNull`; only C callers can pass null). Drop is not instant: QuickFIX's
`ThreadedSocketAcceptor::onStop` busy-waits up to 5 s while a session is logged on, whatever
`force` says — a C++ fork item.
**Lint, measured properly:** the policy's clippy flags run on `quickfix` alone (`--no-deps`,
default features) give 454 findings in the library and 310 in tests. Of the library findings, 89
are on lines the go-live phases wrote or touched (git blame): 28 are `unsafe` itself, which
`-F unsafe_code` forbids and an FFI binding cannot avoid; about 60 are fixable (22 missing SAFETY
comments, 22 doc backticks, 6 missing `# Errors`, a few `must_use`/raw-borrow/semicolon). Not
fixed in this phase; inspection and proposal to the owner.

### Handover 11 — 2026-11-12
**Built:** R-P0-13 test part: `quickfix/tests/test_data_length.rs`, a Logon with `95=<n>|96=x`
for n = 100000, 100000000, 2000000000, -50, three ways: raw TCP peer → MultiThreaded acceptor,
raw TCP listener answering a MultiThreaded initiator's Logon, and in process through
`DataDictionary::try_build_message` (12 tests). Expected once fixed: refused, never logged on,
process alive (the acceptor then still answers a good Logon). The research harness
(`gianghn/ClaudeResearch` `quickfix-rs-golive/harness/`), ported where the repo had no test:
`test_reentry.rs` (`reenter`, `cross`: `on_msg_from_app` replies on the same session / forwards
to the acceptor's other session while 2 client threads per session send 200 News each; every
message and reply must arrive in 30 s, a stall aborts instead of hanging in `stop()`);
`test_soak.rs` (`stress`, `latency`), opt-in, see below. `utils::add_session` for two-session
settings (`build_settings` uses it).
**Red → green:** R-P0-13, recorded per test (each run alone, the crash takes the binary down).
ASan (Linux aarch64, `qfrs-san` image): 11 of 12 red — every 1e5/1e8/2e9 case is a
heap-buffer-overflow in `FieldBase::FieldBase` ← `Message::extractField` ← `setString` (socket
path via `Session::next` ← `ThreadedSocketConnection::processStream`); -50 on both socket sides is
`std::length_error` escaping a QuickFIX thread → terminate (SIGABRT). Plain macOS: only 4 die
(socket -50: SIGABRT; `try_build_message` 1e8/2e9: SIGBUS); the other 1e5/1e8/2e9 socket cases
over-read silently and pass, which is why ASan is the gate. The 11 are `#[ignore = "red until
R-P0-13: …"]` and `address` lines in `scripts/san-known-red.txt`. In process with -50 is green
today (the binding's `catch (...)` turns `length_error` into an error), so it runs un-ignored.
Re-entry and cross-session: green on first run (no deadlock, nothing lost), plain and ASan.
**Opt-in soak (`test_soak.rs`, `#[ignore = "opt-in soak: …"]`):** run with
`QFRS_SOAK_SECS=300 cargo test -p quickfix --release --test test_soak -- --ignored --nocapture`.
Chaos: 2 sender threads on FileStores while a fixed cycle logs out/in, stops/starts and
drops/rebuilds each side; every `Ok` send must arrive once, in order. Measured (macOS, release):
30 s, 36 rounds, 1 493 575 sends, 0 lost/dup/out of order. Latency (memory store, echo from
`on_msg_from_app`), 5000 round trips: p50 57 µs, p90 153, p99 1706, p99.9 10464, max 27396
(a second run of 1000: p50 45, p99 101).
**Already covered, not ported again:** harness `panic` (test_callback_failures), `raw nonutf8`
and `nul` (test_wire_bytes; same byte path for any tag), `raw datalen` / `parse` (R-P0-13 above),
`audit-repro` (swap_header / swap_to_app / `!Sync` app: phase 5 and 7 `compile_fail` doctests;
fix_t_double_free: phase 5; stale_error: test_seqnum; unsupported / fieldnotfound:
test_callback_failures), `api-probe` (43=Y, 150=C, missing 17, wrong 35, NaN price: KRX suite,
phase 8), `run_tests_loop.sh` (`scripts/test-loop.sh`).
**Checks:** `cargo test --workspace` (macOS) 150 passed, 0 failed, 13 ignored (11 R-P0-13 red,
2 opt-in soak); clippy 1.99.1 `--workspace --all-targets -D warnings` exit 0; fmt ok;
`scripts/san.sh address` whole workspace exit 0 (Linux container), and again on the three new
binaries after the last edit. `lint.sh .` exit 1, every failure pre-existing: ast-grep (10
upstream `#[allow]`s in `quickfix-spec-parser` / `quickfix-ffi`, ~15 000 findings in vendored
C++), all 16 cpp-policy projects (vendored, D5), rust policy config/metadata/clippy/rustdoc (stop
in `quickfix-spec-parser`) and test (`--all-features` needs MySQL, the `quickfix-ffi` build
script fails); fmt, deny and msrv pass. None of its findings is in a file this phase changed,
and its clippy never reaches `quickfix`, hence the measurement below.
**Lint, measured as in 10b** (policy flags on `quickfix --tests`, `--no-deps`): the new files
have 33 findings, all `expect` in helper functions (`expect_used` / `unwrap_in_result`): the
policy's `allow-expect-in-tests` only covers `#[test]` functions, not helpers in integration
test files — the same style as `utils/raw_peer.rs` and `test_callback_failures.rs`. Fixed on the
way: SAFETY placement, a 91-line function, nesting, `let _ =` on `must_use`, doc backticks,
`eprintln!`. `settings_builder.rs` keeps its 3 older findings (`ServerType` by value, no `Debug`,
`expect`).
**Decisions taken:** (1) soak tests are `#[ignore]`d, so `cargo test -- --ignored` (D4's way to
see red tests) also runs them; their ignore reason says "opt-in soak", not "red". (2) Known-red
lines only for `address`, the sanitizer the red result was recorded under (TSan is report-only,
D6). (3) `Session::reset` stays out of the chaos cycle: it wipes the store by design (R-P1-7).
**Not ported:** the harness's `fuzz/` target (needs `libfuzzer-sys` and cargo-fuzz: a new
dependency; proposal: a `fuzz/` crate outside the workspace, nightly CI job); its watchdog's
`gdb` stack dumps and `repro_segv_loop.sh` (debug aids, not tests); the api-probe `0.1+0.2`
price print (f64 money, banned by the house rules, see handover 8).
**Left for the owner:** R-P0-13 stays red until the C++ fork cherry-picks #779 (must also refuse
a negative length) and the submodule is bumped; that phase deletes the 11 `#[ignore]`s and
known-red lines.

### Handover 12 — 2026-11-12
**Built:** R-P1-5 rest. (a) `build.rs` makes both CMake builds write deterministic archives:
on Linux `CMAKE_<LANG>_ARCHIVE_CREATE/APPEND/FINISH` use `ar qcD` / `ar qD` / `ranlib -D`; on
macOS (`TargetOs::Other`) the builds get `ZERO_AR_DATE=1`. New
`scripts/check-reproducible-build.sh`: two clean builds (this checkout and a copy at another path,
fresh target dirs), compares `libquickfix.a` and `libquickfixbind.a`, names differing members.
(b) New `scripts/split-debuginfo.sh BINARY` (objcopy `--only-keep-debug`, then `--strip-debug
--add-gnu-debuglink`; Linux) and `scripts/check-split-debuginfo.sh` (release example built with
`debug = "line-tables-only"`, split, then: no `.debug_info` in the binary, `.gnu_debuglink`
present, libquickfix's DWARF in the `.debug` file). Both checks are steps of the CI
`build-hardening` job. `KRX_FORK_NOTES.md` "Shipping a release binary" tells the gateway what to
set and ship. Lint: the 60 fixable policy-clippy findings on go-live lines (22 SAFETY comments
stating the invariant, 22 doc backticks, 6 `# Errors`, 3 `#[must_use]`, 3 `&raw mut`, 2
`let _ =` in `Drop` → `.ok()` with the reason, 2 semicolons). The edit hook rejects
`dictionary_item.rs`'s upstream `#![allow(missing_debug_implementations)]`, so it is gone and
every item, `ConnectionType` and `SSLProtocol` derive `Debug`.
**Red → green:** two clean builds, macOS: both archives differed (headers only, no member
differed) → identical. Linux (Ubuntu 24.04 container, gcc 13, the CI image's distro): identical
before the change already (Ubuntu's binutils defaults to `D`; 10a's `-ffile-prefix-map` covers
paths) and after, same hashes; the `D` rules are in the CMake cache, for distros whose `ar` is not
deterministic by default. Split: a default release binary had 0 debug sections, because Cargo's
`debug = 0` implies `strip = "debuginfo"` and drops libquickfix's `-g` → 1.47 MB stripped
binary + 32.7 MB `.debug` with the C++ debug info (Linux). Policy clippy flags on `quickfix`
(`--lib --no-deps`, default features, `CLIPPY_CONF_DIR` default, blame against `1b42bc5`, measured
on a commit): library 455 → 393; go-live lines 90 → 30 = 29 `unsafe_code` (FFI) + 1
`unnecessary_wraps` (kept, below). The 10b note said 454/89; the same method on this base gives
455/90 (one more `unsafe_code`).
**Checks:** `cargo test --workspace` exit 0, 147 passed, 0 failed (macOS); clippy 1.99.1
`--workspace --all-targets -D warnings` exit 0; `cargo +1.86.0 check --workspace --all-targets
--locked` exit 0; fmt ok; `scripts/check-build-hardening.sh` exit 0 (macOS);
`check-reproducible-build.sh` exit 0 (macOS and Linux); `check-split-debuginfo.sh` exit 0 (Linux).
`lint.sh .` exit 1 as before: every finding is in vendored libquickfix (D5) or in upstream
crates (policy clippy stops at `quickfix-spec-parser`, 40 errors, and never reaches `quickfix`);
none is in a file this phase changed. CI not yet run on the new steps.
**Decisions taken:** split with objcopy and a gateway profile setting, not a workspace
`split-debuginfo` (this workspace's profiles do not reach the gateway binary; Cargo's split covers
Rust only). `Message::from_fix_message_t` keeps its always-`Ok` `Result`: its doc keeps it on
purpose for later validation, and dropping it breaks callers.
**Away from the plan:** none. `quickfix/tests` not touched.
**Left for the owner:** the gateway's release profile needs `debug = "line-tables-only"` and its
release job must run `split-debuginfo.sh` (gateway repo). Production-matching distro image still
open (which distro?). Policy-clippy findings outside go-live lines (363 library, upstream) and in
tests are untouched.

### Handover 13 — 2026-11-12
**Built:** `rocky9` job in `krx_ci.yml` (D12): `rockylinux/rockylinux:9` (9.8) pinned by digest,
`gcc-c++ cmake make git perl-Digest-SHA diffutils` from the Rocky repos before checkout (without
git, checkout fetches no submodules), Rust 1.99.1, then `cargo test --workspace --locked`,
`check-build-hardening.sh` and `check-reproducible-build.sh`. gcc 11.5, binutils 2.35, glibc 2.34.
**Red → green:** Rocky's `ar` is not deterministic by default (Ubuntu's is): without phase 12's
`ar D`/`ranlib -D` both archives differ, with them they are identical. The image has no `cmp`
(added `diffutils`). Local container runs: 150 passed, 0 failed, 13 ignored, arm64 and amd64.
**Away from the plan (lead):** the agent pinned Docker Hub's `rockylinux:9`, deprecated at 9.3;
switched to the maintained `rockylinux/rockylinux:9`, which sets no `PATH` in its image config, so
after rust-toolchain's `GITHUB_PATH` entry the PATH held only `~/.cargo/bin` and `bash` was not
found; the job now sets the standard `PATH` in `container.env`. CI green on PR #18 at `7dbe48c`.
**Left for the owner:** ASan, TSan and SSL run on Ubuntu only. A local cargo build writes
`config.h` into the libquickfix source tree (submodule dirty; a C++ fork item).

### Handover 14 — 2026-11-12
Full note in PR #19. **Built:** `fuzz/` (own workspace and lock, `libfuzzer-sys`), four byte
targets (no dictionary, FIX44, FIX44KRX, FIXT1.1+FIX50 merged spec), 98 seeds, `fix.dict`,
`scripts/fuzz.sh` (libquickfix under `-fsanitize=address,fuzzer-no-link`), nightly
`krx_fuzz.yml` (4 × 10 min, crash artifacts uploaded).
**Red → green:** same target, seeds and dictionary: plain `cargo fuzz` 0 crashes in 65.4 M
inputs, 88 edges; `scripts/fuzz.sh` finds the R-P0-13 heap over-read in 1.27 M inputs (~105 s,
1 099 edges) through `90`/`91` (SecureData), so the C++ fix must cover every data field. Nightly
mode, filter on: 4 targets × 10 min, 0 crashes.
**Decisions:** no new public API in that phase (targets skip non-UTF-8: phase 15 adds the byte
API, owner 12 Nov); FIXT uses the merged FIX50 spec; default feature `skip-r-p0-13` filters the
known crash, `fuzz/known_crashes/` asserts it (delete both when the C++ fix is pinned). NCSA
(libFuzzer's licence) added to the central `deny.toml` by the lead, owner-approved; `rust-policy-check
fuzz --only deny` passes.
**Left for the owner:** corpus caching between nightly runs and coverage reports; the
two-dictionary session parse needs new API.

### Handover 15 — 2026-11-12
Started by an agent, finished by the lead after the agent ran out of context. **Built:**
`FixMessage_fromBytes(ptr, len)` and `FixMessage_fromBytesAndDictionary` in quickfix-bind;
`Message::try_from_bytes`, `DataDictionary::try_build_message_bytes`; fuzz targets parse bytes
(the UTF-8 skip is gone, the `skip-r-p0-13` filter stays). CHANGELOG entry under a new
"Unreleased" section, not 0.3.0.
**Red → green:** `test_parse_bytes.rs` (7) and 5 new `test_data_length.rs` tests did not compile
without the API. After: Latin-1 in 58 and a NUL inside 96 round-trip byte-exact; macOS
158 passed / 0 failed / 16 ignored, MSRV 1.86 builds; ASan container 158/0/2. The three huge-length
byte tests (1e5, 1e8, 2e9) are red under ASan as R-P0-13, `#[ignore]` and in
`scripts/san-known-red.txt`; -50 is refused and runs. Fuzz 4 targets × 60 s via bytes: 0
crashes (≈ 295–346 k runs each). `rust-policy-check fuzz` passes, lint `--changed` exit 0.
**Left for the owner:** none new; the R-P0-13 C++ fix must also cover negative lengths and
90/91.

### Handover 16 — 2026-11-13
**Built:** D17: `libquickfix` → C++ fork `krx/main` @ `11703b2a` (114 commits: its V1 phases
1–5). 14 R-P0-13 tests un-ignored, `scripts/san-known-red.txt` empty, 4 new rows (16.4–16.6);
`fuzz/`: `skip-r-p0-13` and `overreads_data_field` deleted, `known_crashes/` moved into `seeds/`,
one test that a dictionary refuses them. Docs: fork notes (pin, phase 16 row), go-live plan
(R-P0-13 ticked, 11 of 13 P0), gateway handover (risk table, new `.ini` settings), CHANGELOG.
**Red → green:** 17 → 0 of 20 rows under ASan, each test run on its own (old pin: 14 heap
over-reads, 2 `std::length_error` aborts, 2 wrong expectations; checklist 16.1–16.6).
**Checks:** macOS `cargo test --workspace` 176 passed / 0 failed / 2 ignored (was 158/0/16);
ASan container `scripts/san.sh address` 176/0/2, no skips; clippy `-D warnings` and fmt clean
(workspace and `fuzz/`); fuzz 4 targets × 60 s without the filter: 782 k–965 k runs, 0 crashes;
`rust-policy-check fuzz` passes. `lint.sh --changed` fails only on findings outside this phase's
files (upstream `#[allow]`s, `libquickfix/doc/html`, workspace policy metadata,
`--all-features` needing MySQL), as on `krx/main`.
**Measured:** TSan not measured locally: on aarch64 Docker the first TSan binary dies with
SIGSEGV before any test (D3); CI's x86_64 TSan job gives the count for phase 18.
**Decisions taken:** 16.5 corrected, not forced: without a dictionary QuickFIX treats no field as
data, so `try_from_bytes` ends 91/96 at the next SOH (`Ok`, never an over-read) on both pins.
`Session::send` may now return `Ok(false)` (C++ P0-6); no Rust test depended on the old answer.
**Left for the owner:** phase 17 (R-P0-12 API), phase 18 once CI's TSan count is in.
