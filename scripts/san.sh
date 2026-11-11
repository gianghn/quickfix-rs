#!/usr/bin/env bash
# Run the workspace tests under a sanitizer, C++ (libquickfix, quickfix-bind) and Rust alike.
# Needs nightly with rust-src and clang. Linux is the reference platform (CI).
#
# usage: scripts/san.sh <address|thread> [cargo test args...]
#
# Tests listed for this sanitizer in scripts/san-known-red.txt are skipped: they are bugs with a
# go-live item id, still open. Each fix removes its lines (KRX_BUILD_PLAN.md, decision D4).
set -euo pipefail

san=${1:?usage: scripts/san.sh <address|thread> [cargo test args...]}
shift
case "$san" in
  address | thread) ;;
  *)
    echo "san.sh: sanitizer must be address or thread, not $san" >&2
    exit 2
    ;;
esac

root=$(cd "$(dirname "$0")/.." && pwd)
host=$(rustc +nightly -vV | sed -n 's/^host: //p')

flags="-Zsanitizer=$san -Cforce-frame-pointers=yes -Cdebuginfo=1"
# TSan needs an instrumented std (-Zbuild-std), or it reports libtest's own channels as races.
# ASan does not, and runs without it: on aarch64 Linux (nightly 2026-11) -Zbuild-std makes every
# libtest binary crash at start, even for an empty crate. So TSan is checked on x86_64 (CI) only.
build_std=()
if [[ "$san" == thread ]]; then
  build_std=(-Zbuild-std)
fi
export RUSTFLAGS="$flags" RUSTDOCFLAGS="$flags"
export CARGO_TARGET_DIR="$root/target/san-$san"
export CC=clang CXX=clang++
# Symbolized reports name the C++ function; Ubuntu ships the tool as llvm-symbolizer-18.
for sym in llvm-symbolizer llvm-symbolizer-18; do
  if command -v "$sym" >/dev/null; then
    export ASAN_SYMBOLIZER_PATH TSAN_SYMBOLIZER_PATH
    ASAN_SYMBOLIZER_PATH=$(command -v "$sym")
    TSAN_SYMBOLIZER_PATH=$ASAN_SYMBOLIZER_PATH
    break
  fi
done
export CFLAGS="-fsanitize=$san -fno-omit-frame-pointer -g -O1"
export CXXFLAGS="$CFLAGS"
# Abort on the first report so a sanitizer finding always fails the test binary.
export ASAN_OPTIONS="${ASAN_OPTIONS:-abort_on_error=1:detect_leaks=1}"
export TSAN_OPTIONS="${TSAN_OPTIONS:-halt_on_error=1:second_deadlock_stack=1}"

skips=()
while read -r kind name _; do
  if [[ "$kind" == "$san" ]]; then
    skips+=(--skip "$name")
  fi
done <"$root/scripts/san-known-red.txt"

# --target keeps the sanitizer flags off build scripts and proc macros.
cargo +nightly test ${build_std[@]+"${build_std[@]}"} --target "$host" --workspace "$@" -- --exact ${skips[@]+"${skips[@]}"}
