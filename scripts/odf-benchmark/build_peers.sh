#!/bin/sh
# Builds every peer runner into one shared target directory and records which built.
# Usage: build_peers.sh <target dir> [peer ...]
target="$1"; shift
here="$(cd "$(dirname "$0")" && pwd)"
peers="${*:-$(ls "$here/peers")}"
for peer in $peers; do
  start=$(date +%s)
  if (cd "$here/peers/$peer" && CARGO_TARGET_DIR="$target" cargo build --release --ignore-rust-version >"$target/build-$peer.log" 2>&1); then
    echo "BUILT $peer $(( $(date +%s) - start ))s"
  else
    echo "FAILED $peer $(( $(date +%s) - start ))s"
  fi
done
echo ALL_DONE
