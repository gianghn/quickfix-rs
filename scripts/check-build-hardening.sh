#!/usr/bin/env bash
# The C++ build must be hermetic and hardened (R-P1-5):
#  1. with the libquickfix submodule missing, the build stops with a clear message and makes no
#     network call (no `git submodule update` from build.rs);
#  2. libquickfix and quickfix-bind are compiled with warnings on and the hardening flags.
# The stack-clash and FORTIFY flags are expected on Linux only.
#
# usage: scripts/check-build-hardening.sh
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
fail=0

# 1. No submodule: a copy of the repo without libquickfix's files, offline.
# Crates are fetched first: offline must fail on the submodule, not on an empty cargo cache (fresh CI).
(cd "$root" && cargo fetch --locked)
copy=$(mktemp -d)
trap 'rm -rf "$copy"' EXIT
tar -C "$root" --exclude=./target --exclude='./quickfix-ffi/libquickfix/*' --exclude='*/build' \
  -cf - . | tar -xf - -C "$copy"
status=0
out=$(cd "$copy" && CARGO_NET_OFFLINE=true CARGO_TARGET_DIR="$copy/target" \
  cargo build -q -p quickfix-ffi 2>&1) || status=$?
if [[ $status == 0 ]]; then
  echo "FAIL: build succeeded without the libquickfix submodule"
  fail=1
fi
if grep -q 'libquickfix submodule is missing' <<<"$out"; then
  echo "ok: missing submodule stops the build with a clear message"
else
  echo "FAIL: missing submodule did not give the expected message; build output:"
  tail -5 <<<"$out"
  fail=1
fi

# 2. Flags of both CMake builds, from their caches in the newest quickfix-ffi build output.
messages=$(cd "$root" && cargo build -q -p quickfix-ffi --message-format=json)
# OUT_DIR of quickfix-ffi's build script, from cargo's build-script-executed message.
out_dir=$(grep '"reason":"build-script-executed"' <<<"$messages" | grep 'quickfix-ffi' \
  | sed -n 's/.*"out_dir":"\([^"]*\)".*/\1/p')
want=(-Wall -fno-omit-frame-pointer -fstack-protector-strong -ffile-prefix-map=)
os=$(uname -s)
if [[ "$os" == Linux ]]; then
  want+=(-fstack-clash-protection -D_FORTIFY_SOURCE=2)
fi
for cache in "$out_dir/build/CMakeCache.txt" "$out_dir/libquickfix-out/build/CMakeCache.txt"; do
  if [[ ! -f "$cache" ]]; then
    echo "FAIL: no CMake cache at $cache"
    fail=1
    continue
  fi
  flags=$(sed -n 's/^CMAKE_CXX_FLAGS:STRING=//p' "$cache")
  for flag in "${want[@]}"; do
    if [[ " $flags" != *" $flag"* ]]; then
      echo "FAIL: $flag missing from $cache"
      fail=1
    fi
  done
  if [[ " $flags " == *" -w "* ]]; then
    echo "FAIL: -w (all warnings hidden) in $cache"
    fail=1
  fi
done
if [[ $fail == 0 ]]; then
  echo "ok: hardening flags present, warnings on"
fi
exit "$fail"
