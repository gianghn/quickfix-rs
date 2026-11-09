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
(see `KRX_FORK_NOTES.md` there for the C++ fixes), pinned at `33b12daf`. Upstream pins QuickFIX
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
| 9b395d2 `quickfix-msgkrx` crate (FIX44KRX.xml) | **kept** | The KRX dictionary the gateway uses. Now version 0.2.2, `publish = false`. |
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

Removed with the rebase: the `experimental-multi-thread-support` feature. Upstream builds
`MultiThreaded` always. The gateway enables that feature in its `Cargo.toml`, so its manifest has to
drop it (and move its version requirements from 0.1.6 to 0.2.2) when it takes `krx/main`.

## The threading flag: decision pending (owner)

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
