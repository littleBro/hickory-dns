#!/usr/bin/env bash
# Compare two refs with the review's property test and stable-toolchain timings.
#
#   dev/memory-series-review/compare.sh <base-ref> <series-ref> [passes]
#
# Each ref gets its own worktree and its own target directory: cargo hashes
# artifacts by package id relative to the workspace root, so worktrees of the
# same repository collide in a shared target directory and the second one
# silently reuses the first one's build.
set -euo pipefail

base=${1:?base ref}
series=${2:?series ref}
passes=${3:-2}
here=$(cd "$(dirname "$0")" && pwd)
root=$(git -C "$here" rev-parse --show-toplevel)
work=${WORK:-/tmp/memory-series-review}
mkdir -p "$work"

setup() {
    local name=$1 ref=$2 wt="$work/$1"
    if [ ! -d "$wt" ]; then
        git -C "$root" worktree add -q "$wt" "$ref"
    fi
    cp "$here/review_props.rs" "$wt/crates/proto/tests/review_props.rs"
    cp "$here/review_bench.rs" "$wt/crates/proto/tests/review_bench.rs"
    echo "$name: $(git -C "$wt" log --oneline -1)"
}

setup base "$base"
setup series "$series"

for name in base series; do
    if (cd "$work/$name" && CARGO_TARGET_DIR="$work/target-$name" \
        cargo test -p hickory-proto --test review_props -- --nocapture --test-threads=1 \
        > "$work/props-$name.log" 2>&1); then
        echo "props $name: ok ($(grep -o 'SIZES.*' "$work/props-$name.log"))"
    else
        echo "props $name: FAILED, see $work/props-$name.log"
    fi
done

for pass in $(seq 1 "$passes"); do
    for name in base series; do
        (cd "$work/$name" && CARGO_TARGET_DIR="$work/target-rel-$name" \
            cargo test --release -p hickory-proto --test review_bench -- \
            --ignored --nocapture --test-threads=1 > "$work/bench-$name-$pass.log" 2>&1)
        echo "bench $name pass $pass: done"
    done
done

echo
echo "label | $(for pass in $(seq 1 "$passes"); do for name in base series; do printf '%s#%s | ' "$name" "$pass"; done; done)"
grep -o 'BENCH [^:]*' "$work/bench-base-1.log" | sed 's/^BENCH //' | while IFS= read -r label; do
    line="$label"
    for pass in $(seq 1 "$passes"); do
        for name in base series; do
            value=$(grep -F "BENCH $label:" "$work/bench-$name-$pass.log" | sed 's/.*: //' || true)
            line="$line | ${value:-?}"
        done
    done
    echo "$line"
done
