#!/usr/bin/env bash
# Moves a release binary's debug info (Rust and libquickfix's C++ -g) into BINARY.debug and
# leaves BINARY stripped, with a .gnu_debuglink to the .debug file so gdb and perf find it.
# Ship BINARY; keep BINARY.debug with the release (R-P1-5). Linux (ELF) only.
#
# usage: scripts/split-debuginfo.sh BINARY
set -euo pipefail

bin=${1:?usage: scripts/split-debuginfo.sh BINARY}
os=$(uname -s)
if [[ "$os" != Linux ]]; then
  echo "split-debuginfo.sh: Linux only (macOS keeps debug info apart with dsymutil)" >&2
  exit 2
fi
objcopy --only-keep-debug "$bin" "$bin.debug"
chmod -x "$bin.debug"
objcopy --strip-debug --add-gnu-debuglink="$bin.debug" "$bin"
