#!/usr/bin/env bash
# A release binary splits into a stripped binary and a .debug file holding the C++ debug info
# (R-P1-5). Linux only.
#
# usage: scripts/check-split-debuginfo.sh
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# The profile KRX_FORK_NOTES.md asks the gateway for: with Cargo's default (`debug = 0`, hence
# `strip = "debuginfo"`) the link drops libquickfix's debug info and there is nothing to split.
(cd "$root" && CARGO_PROFILE_RELEASE_DEBUG=line-tables-only \
  cargo build -q --release --locked -p quickfix --example fix_getting_started)
bin="$work/fix_getting_started"
cp "$root/target/release/examples/fix_getting_started" "$bin"
"$root/scripts/split-debuginfo.sh" "$bin"

fail=0
readelf -S "$bin" >"$work/bin.sections"
if grep -q '\.debug_info' "$work/bin.sections"; then
  echo "FAIL: the shipped binary still has .debug_info"
  fail=1
fi
if ! grep -q '\.gnu_debuglink' "$work/bin.sections"; then
  echo "FAIL: the shipped binary has no .gnu_debuglink"
  fail=1
fi
# libquickfix is built RelWithDebInfo: its sources must be in the .debug file's line tables.
readelf --debug-dump=info "$bin.debug" >"$work/debug.info" 2>/dev/null
if ! grep -q 'Session\.cpp' "$work/debug.info"; then
  echo "FAIL: no libquickfix debug info (Session.cpp) in the .debug file"
  fail=1
fi
if [[ $fail == 0 ]]; then
  bin_size=$(stat -c %s "$bin")
  debug_size=$(stat -c %s "$bin.debug")
  echo "ok: stripped binary $bin_size bytes, .debug file $debug_size bytes with the C++ debug info"
fi
exit "$fail"
