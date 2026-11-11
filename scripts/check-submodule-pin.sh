#!/usr/bin/env bash
# Fail unless libquickfix comes from our fork (gianghn/quickfix) and its pinned commit is on the
# fork's krx/main, so the gateway never builds a C++ commit nobody reviewed there.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
sub=quickfix-ffi/libquickfix
want_url=https://github.com/gianghn/quickfix.git
want_branch=krx/main

url=$(git -C "$root" config -f .gitmodules "submodule.$sub.url")
branch=$(git -C "$root" config -f .gitmodules "submodule.$sub.branch")
if [[ "$url" != "$want_url" || "$branch" != "$want_branch" ]]; then
  echo "submodule $sub tracks $url ($branch), want $want_url ($want_branch)" >&2
  exit 1
fi

pin=$(git -C "$root" rev-parse "HEAD:$sub")
git -C "$root/$sub" fetch --quiet origin "$want_branch"
if ! git -C "$root/$sub" merge-base --is-ancestor "$pin" FETCH_HEAD; then
  echo "submodule $sub is pinned at $pin, which is not on $want_branch of $want_url" >&2
  exit 1
fi
echo "submodule $sub pinned at $pin, on $want_branch"
