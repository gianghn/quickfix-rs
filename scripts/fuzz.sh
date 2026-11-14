#!/usr/bin/env bash
# Run cargo-fuzz on the fuzz/ crate with libquickfix instrumented as well.
# Needs nightly, cargo-fuzz and clang. Linux is the reference platform (CI).
#
# usage: scripts/fuzz.sh <cargo fuzz args...>
#   scripts/fuzz.sh run parse_fix44 -- -max_total_time=600
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
host=$(rustc +nightly -vV | sed -n 's/^host: //p')

# cargo-fuzz instruments only Rust. The parser is C++: without ASan and libFuzzer's coverage
# counters there, the fuzzer is blind inside it and misses its memory errors.
export CC=clang CXX=clang++
export CFLAGS="-fsanitize=address,fuzzer-no-link -fno-omit-frame-pointer -g -O1"
export CXXFLAGS="$CFLAGS"
# libfuzzer-sys builds libFuzzer itself with cc, which prefers the target-suffixed variables:
# empty ones keep libFuzzer uninstrumented. quickfix-ffi's build script reads the plain ones.
export "CFLAGS_${host//-/_}=" "CXXFLAGS_${host//-/_}="
for sym in llvm-symbolizer llvm-symbolizer-18; do
  if command -v "$sym" >/dev/null; then
    export ASAN_SYMBOLIZER_PATH
    ASAN_SYMBOLIZER_PATH=$(command -v "$sym")
    break
  fi
done

cd "$root/fuzz"
# A prebuilt cargo-fuzz builds for the triple it was built for (musl from install-action), where ASan
# can't link; build for the host, which the target-suffixed flags above assume too.
case ${1:-} in
  run | build) set -- "$1" --target "$host" "${@:2}" ;;
  *) ;;
esac
exec cargo +nightly fuzz "$@"
