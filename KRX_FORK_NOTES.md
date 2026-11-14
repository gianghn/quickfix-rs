# KRX fork notes

This fork (`gianghn/quickfix-rs`) is what the KRX gateway (`enCapital/krx_gateway_rust`) vendors
at `vendor/quickfix-rs`. It carries a few patches on top of upstream `arthurlm/quickfix-rs`.
Keep this file up to date when a patch is added, sent upstream or dropped.

## Branches

| Branch / tag | What it is |
|---|---|
| `main` | Mirror of upstream `main`. Never commit to it; fast-forward it from upstream. |
| `krx/main` | Integration branch: upstream release + our patches. The gateway vendors this. |
| `krx/<topic>` | Topic branches, merged into `krx/main` by PR (CI: `.github/workflows/krx_ci.yml`). |
| `thread-mod` | The old integration branch, **frozen**: gateway commits pin SHAs on it, so it is never rewritten. Its last commit is tagged `krx/thread-mod-final` (6b9c156). |

`krx/main` was started at upstream `cc2b7a5` (release 0.2.2, 2026-10). The submodule
`quickfix-ffi/libquickfix` points at our QuickFIX fork `gianghn/quickfix`, branch `krx/main`
(see `KRX_FORK_NOTES.md` there for the C++ fixes), pinned at `fc866418` since go-live phase 21a
(`e1b1559e` in phase 19, `11703b2a` in phase 16, `33b12daf` before). Upstream pins QuickFIX
master `386ce46e`; on a sync, ours wins.

## Our patches on upstream 0.2.2

The 17 commits of `thread-mod` (since merge base `1ee16fe`, 2025-05-29) and what happened to each
in the rebase on 0.2.2:

| thread-mod commit | Decision | Why |
|---|---|---|
| cfc97de rename Socket(Acceptor\|Initiator) to Acceptor/Initiator | replaced | Upstream f71a9b6 (#26) has the same rename. |
| cd5f370 build ThreadedSocket* C++ files | replaced | Upstream f71a9b6. |
| 7454ba3 docs: ABOUT.md | replaced | Upstream f71a9b6 / 68d6eb9 rewrote it. |
| ee71e8a multi-threaded connection mode | replaced | Upstream f71a9b6, plus the flag fix 37bcedd / ea48231 (see below). |
| 80f8ac1 rust-fmt | dropped | Formatting of code replaced above. |
| 732666a rename ConnectionMode to FixSocketServerKind | replaced | Upstream uses the same name. |
| cb6d476 lifetime clippy warnings | replaced | Same change already upstream. |
| cba08af rust fmt | dropped | Formatting of code replaced above. |
| f7982ea `Session::set_next_{sender,target}_msg_seq_num` | **kept** | Gateway operator commands (`src/port_fix/control.rs`). Not upstream. |
| b2c8b29 `Session::get_expected_{sender,target}_num` + tests | **kept** | Same. Not upstream. |
| 9b395d2 `quickfix-msgkrx` crate (FIX44KRX.xml) | **kept** | The KRX dictionary the gateway uses. Now version 0.4.1 with the workspace, `publish = false`. |
| 36098d7 `Message::get_fix_message_t` | **kept** | Gateway converter. Not upstream. |
| f198bed `Message::from_fix_message_t` | **kept** | Same. Not upstream. |
| 00132e0 `Message::to_fix_bytes` | **kept, thin wrapper** | Upstream added `to_fix_raw_bytes` (dc3d581), same two C++ calls, but it keeps the trailing NUL. The gateway's message path calls `to_fix_bytes` and wants no NUL, so `to_fix_bytes` now only pops that NUL. |
| e654c25 quick-xml 0.37 to 0.41 (RUSTSEC-2026-0194/0195) | replaced | Upstream 182024e: quick-xml 0.42. |
| 3a16e01 Cargo.lock for quick-xml and msgkrx | replaced | Upstream lock + our msgkrx entry. |
| 6b9c156 libquickfix from our fork; `krx_ci.yml` | **kept, adapted** | Submodule now `branch = krx/main` at 33b12daf; CI runs on `krx/**`, tests in parallel. |

Added in the rebase: `quickfix/tests/test_session.rs` holds `GLOBAL_LOCK` like upstream's
47da09c does for `test_send_receive`, and waits until **both** sides are logged on. Before, its two
tests shared QuickFIX's session registry in parallel and crashed (macOS: 5 of 5 runs failed, 3 with
SIGSEGV); after, 10 of 10 parallel runs pass. Worth sending upstream.

Go-live fixes on `krx/main` (`KRX_BUILD_PLAN.md`; each in `quickfix/CHANGELOG.md`):

| Phase | Change | Gateway impact |
|---|---|---|
| 1 | Sanitizer CI; last-error buffer no longer leaks per thread | none |
| 3 | A panic in any application or log callback aborts the process; `UnsupportedMessageType` sends 35=j 380=3; SSL stub returns an error | run under a supervisor that restarts it; `panic = "abort"` in its profiles matches this |
| 4 | Group copies (`clone_group`, generated group getters) no longer read past the heap | none |
| 5 | Callback traits require `Send + Sync`; `from_fix_message_t` is `unsafe`, new `into_fix_message_t` | app and log types must be `Send + Sync`; wrap `from_fix_message_t` in `unsafe` with a stated pointer origin |
| 6 | `MultiThreaded` is the default, `SingleThreaded` deprecated; dropping an acceptor/initiator waits for in-flight sends | ask for `MultiThreaded`; a send racing a drop now returns `SessionNotFound` instead of crashing |
| 7 | Borrowed views: `MessageMut`, `HeaderMut`, `TrailerMut`, `GroupMut` | `on_msg_to_admin`/`on_msg_to_app` take `&mut MessageMut<'_>`; helper fns typed `&mut Header`/`&Group` change to the view types |
| 8 | Generated `try_get_*` and `TryFrom<Message>` (`FieldError`); Y/N booleans parse; NaN/inf floats rejected | `X::from(msg)` becomes `X::try_from(msg)?`; prefer `try_get_*` on inbound messages; custom `IntoFixValue` impls return `QuickFixError` |
| 9 | Seqnum APIs take/return `u64`/`Result`; errors are never stale; log callbacks and `get_field_bytes` give exact bytes; `to_fix_bytes` keeps bytes after a NUL | operator seqnum commands pass `u64` and handle `Err`; override `LogCallback::on_*_bytes` for an exact audit log |
| 10a | Hermetic, hardened C++ build (no git fetch, warnings on, stack protector, FORTIFY); CI: MSRV, release `--locked`, audit, SSL, clippy `-D warnings`, SHA-pinned actions; reset runbook | check out the submodule before building (`git submodule update --init --recursive`); set `QUICKFIX_FFI_COMPILER_LAUNCHER=sccache` to keep sccache; operators read `doc/RUNBOOK_SEQNUM.md` |
| 10b | Rejects carry tag/text; `stop_force` (used by `Drop`); `Session::is_enabled`/`refresh`/`send_test_request`; `Dictionary::get` errors instead of `Ok(0)`; `u32` socket buffers; `TimestampPrecision`; `unsafe trait FfiMessageStoreFactory` | `MsgFromAppError::IncorrectTagValue` → `IncorrectTagValue { tag }` (same for `FieldNotFound`, `IncorrectDataFormat`, `RejectLogon { text }`); a custom store factory needs `unsafe impl`; drop no longer waits 10 s for Logout answers (call `stop()` first for a graceful stop) |
| 15 | Byte-level parse: `Message::try_from_bytes`, `DataDictionary::try_build_message_bytes` (non-UTF-8 and NULs pass through); fuzz targets use them | none (additive); parse stored or replayed frames from `&[u8]` with these instead of `try_from_text`/`try_build_message`, which refuse a NUL and need UTF-8. A frame with an over-long data length still over-reads until R-P0-13 |
| 16 | `libquickfix` pinned at C++ fork `krx/main` @ `11703b2a`: R-P0-13 fixed (a data length out of the frame or negative is refused, in every data field), `FIX::Mutex` recursive and `SessionState` flags atomic, plain acceptor closes each socket once, `Session::send` false when stored but not written; new settings `SendTimeout`, `FileStoreSync` | no code change. `Session::send` now returns `Ok(false)` when the message was stored but not written (handle it like a down session). `.ini`: consider `FileStoreSync=Y`, `SendTimeout` (default half of HeartBtInt), `ReconnectInterval` 1–2 s for a 5 s failover. `send_to_target` still returns `Ok` until phase 17 |
| 19 | `Message::as_message_mut()`: an owned message as the `MessageMut` a callback takes; `libquickfix` pinned at `e1b1559e` (C++ teardown fixes 6.1, 6.2, 6.4, 6.5) | none (additive); unit-test `on_msg_to_app`/`on_msg_to_admin` by calling them with `&mut msg.as_message_mut()` instead of moving their logic into helper methods. `on_logout` now runs outside QuickFIX's session lock |
| 17 | `send_to_target` returns `QuickFixError::NotSent` when the message was not written now (R-P0-12) | **breaking:** a send while logged off, a failed write, or `DoNotSend` from `on_msg_to_app` is now an error, not `Ok`; report the order to the OMS as not sent. A stored message may still go out later as a PossDup resend, so keep the stale-resend check in `on_msg_to_app`. Add `NotSent` to any exhaustive `match` on `QuickFixError` |
| 20 | `SessionId: Send + Sync` | none (additive); keep `session.clone()` from `on_logon` in the app (e.g. `Mutex<Option<SessionId>>`) instead of its text, and send through it from any thread |
| 21a | `libquickfix` pinned at `fc866418` (C++ phases 6–8 and 15): background store flush, `MaxQueuedMessages`, faster file store, GapFill/queue, nested group count and resend fixes | no code change. `.ini`: `FileStoreSync=N` with `FileStoreFlushInterval=5` (production, D23); review `MaxQueuedMessages` (default 10000). A KRX message with a wrong nested group count is now rejected |
| 21b | `send_batch_to_target`: a batch under one session lock and one store sync (C++ 15.7) | none (additive); only worth it with `FileStoreSync=Y`: one sender thread per session drains the order queue into one call (at most 64), never waiting to fill a batch; orders from the returned count on were not sent |
| 22 | A successful binding call no longer clears the error text (only a set one is cleared); R-P1-2's behaviour unchanged | none (faster): about 1.5 ns less per FFI call, e.g. a header `get_field` + `set_field` 39.5 → 36.6 ns on Linux |
| 23 | The binding's C++ cleaned up for the C++ policy: the catch chain in three helpers, error state behind accessors, no naked `new` | none: same error codes, same speed |

Removed with the rebase: the `experimental-multi-thread-support` feature. Upstream builds
`MultiThreaded` always. The gateway enables that feature in its `Cargo.toml`, so its manifest has to
drop it (and move its version requirements from 0.1.6 to 0.3.0, the version after the go-live
phases' breaking changes, D2) when it takes `krx/main`.

## The threading flag: decided (owner, 12 Nov 2026)

`MultiThreaded` is the default and `SingleThreaded` is `#[deprecated]` since go-live phase 6
(`KRX_BUILD_PLAN.md`, D1). The gateway asks for `MultiThreaded` explicitly. History:

`thread-mod` passed `is_single_threaded()` into the C++ flag `isMultiThreaded`, so
`FixSocketServerKind::SingleThreaded` built QuickFIX's `ThreadedSocketAcceptor/Initiator`: the
classes our QuickFIX fork fixed and tests (descriptor double close, partial send). Upstream fixed
the swap (37bcedd, ea48231; arthurlm/quickfix-rs#36), and `krx/main` takes that fix.

So on `krx/main`, `SingleThreaded` builds the plain `SocketAcceptor/Initiator`, which still has an
unfixed triple close of its listening socket (see the QuickFIX fork's notes). The gateway asks for
`SingleThreaded` today. To keep running the same C++ classes after it takes `krx/main`, the gateway
must ask for `FixSocketServerKind::MultiThreaded` instead (`src/port_fix/runner.rs`,
`tests/support/src/connection_safety.rs`, `tests/support/src/checker.rs`). That gateway change is
the owner's decision and is not made here.

## Shipping a release binary (R-P1-5)

The static libraries are reproducible: two clean builds give byte-identical `libquickfix.a` and
`libquickfixbind.a` (`scripts/check-reproducible-build.sh`, in CI). libquickfix is always built
`RelWithDebInfo`, but Cargo's default release profile (`debug = 0`, hence `strip = "debuginfo"`)
drops that debug info at link time, so today's release binary has none. To keep it, the gateway's
release profile sets `debug = "line-tables-only"` (Rust line tables plus the C++ debug info; Cargo
then does not strip). Split it off before shipping: `scripts/split-debuginfo.sh <binary>` leaves
the binary stripped with a `.gnu_debuglink` and writes `<binary>.debug`; ship the binary, archive
the `.debug` file with the release (gdb and perf find it next to the binary or under
`/usr/lib/debug`). `scripts/check-split-debuginfo.sh` runs this in CI. Cargo's `split-debuginfo`
setting is not enough: it splits only the Rust code's debug info.

## Syncing from upstream next time

1. Fast-forward `main` from upstream (`gh repo sync gianghn/quickfix-rs -b main`).
2. Branch `krx/sync-<version>` from `krx/main` and merge the new upstream release commit into
   it (a merge, not a rebase: gateway commits pin `krx/main` SHAs, so it is never rewritten).
   Where upstream now has one of our patches, take upstream's and remove ours; update the table
   above.
3. Keep `.gitmodules` on `gianghn/quickfix` / `krx/main` and the submodule at our pinned commit.
4. Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets`, `cargo test --workspace`
   (several times: the socket tests are timing-sensitive).
5. In a gateway worktree, point `vendor/quickfix-rs` at the topic branch and run the gateway's
   definition of done (`dod.sh`), its benchmarks against `core0` and `step0`,
   `cargo test --test test_fd_double_close --test test_partial_send` and the
   `fix_connection_safety.feature` scenarios.
6. Open a PR into `krx/main`; the owner merges it. Then point the gateway's submodule at the
   merged commit in its own PR.

This first move (0.2.2) was a rebase, because `thread-mod` had diverged too far to merge cleanly:
`krx/main` was created at the upstream release and our patches went on top in
`krx/rebase-0.2.2`, leaving `thread-mod` untouched.
