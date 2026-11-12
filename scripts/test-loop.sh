#!/usr/bin/env bash
# Run the socket tests N times; fail if any run fails. One green run proves little for them (R-P0-8).
#
# usage: scripts/test-loop.sh [runs] [cargo test args...]
set -euo pipefail

runs=${1:-20}
shift || true
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

cargo test -p quickfix --test test_socket --test test_send_receive --test test_session --no-run "$@"
failed=0
for i in $(seq 1 "$runs"); do
  if cargo test -q -p quickfix --test test_socket --test test_send_receive --test test_session "$@" >"$root/target/test-loop-$i.log" 2>&1; then
    echo "run $i/$runs ok"
  else
    echo "run $i/$runs FAILED (target/test-loop-$i.log):"
    tail -n 30 "$root/target/test-loop-$i.log"
    failed=$((failed + 1))
  fi
done
echo "$failed of $runs runs failed"
[[ "$failed" -eq 0 ]]
