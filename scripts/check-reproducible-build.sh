#!/usr/bin/env bash
# Two clean builds of the C++ side give byte-identical static libraries (R-P1-5).
# Build A runs in this checkout, build B in a copy at another path, each with its own fresh target
# directory, so embedded paths, timestamps and archive metadata all show up as a difference.
#
# usage: scripts/check-reproducible-build.sh
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

mkdir "$work/copy"
tar -C "$root" --exclude=./target -cf - . | tar -xf - -C "$work/copy"

build() { # checkout, target dir; prints the OUT_DIR of quickfix-ffi's build script
  (cd "$1" && CARGO_TARGET_DIR="$2" cargo build -q --locked -p quickfix-ffi --message-format=json) \
    | grep '"reason":"build-script-executed"' | grep 'quickfix-ffi' \
    | sed -n 's/.*"out_dir":"\([^"]*\)".*/\1/p'
}
show_members() { # archive A, archive B; names the members that differ, to find the cause
  local name
  name=$(basename "$1")
  mkdir -p "$work/x/$name/a" "$work/x/$name/b"
  (cd "$work/x/$name/a" && ar x "$1")
  (cd "$work/x/$name/b" && ar x "$2")
  ar t "$1" >"$work/x/$name/a.list"
  ar t "$2" >"$work/x/$name/b.list"
  cmp -s "$work/x/$name/a.list" "$work/x/$name/b.list" || echo "  member order differs"
  while read -r member; do
    cmp -s "$work/x/$name/a/$member" "$work/x/$name/b/$member" || echo "  member differs: $member"
  done <"$work/x/$name/a.list"
}

out_a=$(build "$root" "$work/target-a")
out_b=$(build "$work/copy" "$work/target-b")

fail=0
for lib in libquickfix-out/lib/libquickfix.a lib/libquickfixbind.a; do
  if [[ ! -f "$out_a/$lib" || ! -f "$out_b/$lib" ]]; then
    echo "FAIL: $lib not built"
    fail=1
    continue
  fi
  a=$(shasum -a 256 "$out_a/$lib" | cut -d' ' -f1)
  b=$(shasum -a 256 "$out_b/$lib" | cut -d' ' -f1)
  if [[ "$a" == "$b" ]]; then
    echo "ok: $(basename "$lib") identical ($a)"
  else
    echo "FAIL: $(basename "$lib") differs: $a vs $b"
    show_members "$out_a/$lib" "$out_b/$lib"
    fail=1
  fi
done
exit "$fail"
