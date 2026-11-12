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
| D2 | Version after the breaking changes | `0.3.0` on `krx/main` at the end of phase 8; gateway moves in one PR | phase 8 |
| D3 | Sanitizer toolchain in CI | nightly `-Zsanitizer`, clang 18 for the C++ side, Linux x86_64. ASan without `-Zbuild-std` (verified locally on aarch64 Linux). TSan with `-Zbuild-std`, because an uninstrumented std makes TSan report libtest's own channels as races; `-Zbuild-std` crashes every libtest binary on aarch64 and TSan cannot run under x86_64 emulation, so the TSan job is first verified by CI | phase 1 |
| D4 | Red tests before their fix | under a sanitizer: listed in `scripts/san-known-red.txt` and skipped by `scripts/san.sh`; in plain `cargo test`: `#[ignore = "red until <item>: why"]`, and `cargo test -- --ignored` shows them red. Each fix phase removes its entries; none left by phase 9 | phase 1 |
| D5 | Lint scope | **Decided by the owner, 12 Nov 2026.** New files must pass `lint.sh`. `quickfix-ffi/quickfix-bind` and `libquickfix` count as vendored upstream code: changed lines follow the house style where they can, the files are not reformatted, and the hook's findings on them are reported in each handover. Whether to exempt vendored C++ in the central policy is the owner's later call | every phase |
| D6 | TSan gate | **Decided by the owner, 12 Nov 2026.** The first CI TSan run found a race in QuickFIX's own `FIX::Mutex::lock()` (unsynchronised read of the owner in its recursive lock; `Initiator::isLoggedOn` vs `connect`). Until a C++ pin fixes it, the TSan job is report-only (`continue-on-error`, `halt_on_error=0`) and lists every distinct race in its job summary; C++ races go to the C++ fork, Rust-side races are fixed here. ASan stays blocking. TSan becomes blocking again after the bump | phase 1 |

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
| 9 | `krx/golive-p9` | done, PR pending | stale error code -15, set(0) accepted, log/`to_fix_bytes` lost bytes after NUL → all green, ASan exit 0 |
| 10 | `krx/golive-p10` | not started | |

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
