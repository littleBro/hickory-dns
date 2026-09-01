# Memory-footprint series: review artifacts

Working material from the review of the memory-footprint series
(tracking issue #5, PRs #1, #2, #3, #4). Nothing here is part of the
workspace; it is kept so the measurements can be repeated when the
series is reworked.

The durable parts went elsewhere:

- the property test for `Name` is a PR of its own
  (`crates/proto/tests/name_props.rs`), so it must not be edited here;
- the benchmark shapes are a PR of their own
  (`crates/proto/benches/name.rs`, `crates/proto/benches/message.rs`).

## Contents

| Path | What it is |
|---|---|
| `review_props.rs` | The property test exactly as it was run for the review, with the original large case counts and the `print_sizes` probe. The checked-in `name_props.rs` is this file with smaller counts and a scale knob. |
| `review_bench.rs` | Timing probes on the stable toolchain: `Instant`-based, `#[ignore]` tests, min of five runs. Crude, but they run without nightly and were what produced the numbers in the reviews. |
| `size_probe/` | A two-line crate that prints `size_of` for `TinyVec<[u8; N]>` at several `N`; how the "44 bytes cost the same as 40" remark was checked. |
| `compare.sh` | Runs `review_props.rs` and `review_bench.rs` on two refs in separate worktrees and prints the bench lines side by side. |
| `results/` | Logs from the review run of 2026-09-01 (main `6c18ce3` vs the four PR branches merged). |
| `RESULTS.md` | The tables distilled from those logs. |
| `corpus/` | Reserved for the name corpus used in the PR descriptions; see the README there. |

## How to repeat

```sh
# property test and stable-toolchain timings, main vs a series ref
dev/memory-series-review/compare.sh origin/main <series-ref>

# the checked-in test with ten times the cases
HICKORY_NAME_PROPS_SCALE=10 cargo test -p hickory-proto --test name_props

# the checked-in benches (nightly)
RUSTFLAGS="--cfg=nightly" cargo +nightly bench -p hickory-proto --bench name --bench message
```

## Lessons that cost time

- **One target directory per worktree.** Cargo derives artifact hashes
  from the package id relative to the workspace root, so two worktrees of
  this repository produce identical hashes. With a shared
  `CARGO_TARGET_DIR` the second worktree reuses the first one's build and
  the comparison silently measures the same code twice. `compare.sh`
  uses `target-<name>` per worktree for that reason.
- **Bench in a quiet machine.** Two interleaved passes agree within 5%
  when nothing else runs; a concurrent `cargo build` moves numbers by
  10% or more.
- **The first `BENCH` line of each test shares a line with the harness's
  `test ... ` prefix** under `--nocapture`, so grep for `BENCH` without
  anchoring to the start of the line.
- `Label::from_ascii` rejects a leading hyphen and the `from_utf8` path
  lowercases through IDNA, so text round trips must generate labels
  accordingly; both are pre-existing behavior, not the series.
