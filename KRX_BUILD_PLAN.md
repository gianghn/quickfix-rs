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
| D1 | Threading default (R-P0-8, pending in `KRX_FORK_NOTES.md`) | `MultiThreaded` becomes the default; `SingleThreaded` stays, marked `#[deprecated]` | phase 6 |
| D2 | Version after the breaking changes | `0.3.0` on `krx/main` at the end of phase 8; gateway moves in one PR | phase 8 |
| D3 | Sanitizer toolchain in CI | nightly `-Zsanitizer`, clang 18 for the C++ side, Linux x86_64. ASan without `-Zbuild-std` (verified locally on aarch64 Linux). TSan with `-Zbuild-std`, because an uninstrumented std makes TSan report libtest's own channels as races; `-Zbuild-std` crashes every libtest binary on aarch64 and TSan cannot run under x86_64 emulation, so the TSan job is first verified by CI | phase 1 |
| D4 | Red tests before their fix | under a sanitizer: listed in `scripts/san-known-red.txt` and skipped by `scripts/san.sh`; in plain `cargo test`: `#[ignore = "red until <item>: why"]`, and `cargo test -- --ignored` shows them red. Each fix phase removes its entries; none left by phase 9 | phase 1 |
| D5 | Lint scope | **Decided by the owner, 12 Nov 2026.** New files must pass `lint.sh`. `quickfix-ffi/quickfix-bind` and `libquickfix` count as vendored upstream code: changed lines follow the house style where they can, the files are not reformatted, and the hook's findings on them are reported in each handover. Whether to exempt vendored C++ in the central policy is the owner's later call | every phase |

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
| 1 | `krx/golive-p1` | done locally, not pushed | ASan: 6 overflow tests + 11 leaking tests → leaks fixed, 6 on known-red (R-P0-7); TSan unverified until CI |
| 2 | `krx/golive-p2` | done locally, not pushed | 8 tests: 6 green, 1 red under ASan (R-P0-7), 1 red `#[ignore]` (R-P0-9) |
| 3 | `krx/golive-p3` | not started | |
| 4 | `krx/golive-p4` | not started | |
| 5 | `krx/golive-p5` | not started | |
| 6 | `krx/golive-p6` | not started | |
| 7 | `krx/golive-p7` | not started | |
| 8 | `krx/golive-p8` | not started | |
| 9 | `krx/golive-p9` | not started | |
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
