# Review results, 2026-09-01

Base: `main` at `6c18ce3`. Series: #1 `444f1ad`, #2 `25e4e6e`, #3 `9009788`,
#4 `b7d510b` merged onto `main` in that order (no conflicts). One shared
4-vCPU VM, stable Rust 1.94.1, release profile for timings. Timings are
the minimum of five runs of N iterations; two interleaved passes agreed
within about 5% unless noted. Raw logs are in `results/`.

## Sizes (x86-64, bytes)

| Type | `main` | series |
|---|---:|---:|
| `Name`, `LowerName` | 80 | 56 |
| `RData` | 184 | 40 |
| `Record` | 272 | 104 |
| `Message` | 152 | 152 |

## Message clone, parse, emit (ns)

| Operation | `main` | series |
|---|---:|---:|
| clone, 4 A answers | 189 | 101 |
| clone, CNAME + A | 115 | 84 |
| clone, NXDOMAIN with SOA in authority | 98 | 77 |
| clone, referral 4 NS + 4 A | 335 | 192 |
| parse A answer, 97 B | 583 | 371 |
| parse CNAME answer, 100 B | 465 | 328 |
| parse NXDOMAIN, 85 B | 354 | 299 |
| parse referral, 169 B | 1243 | 841 |
| parse 40 CNAMEs, 1243 B | 8084 | 7323 |
| emit, all shapes | | 5–12% faster |

Contribution of #3 alone (series with and without it, two passes each,
noise about 10%): A answer 433–485 vs 375–393, CNAME 422–473 vs 325–347,
referral 993–1102 vs 879–961, 40 CNAMEs 8771–9457 vs 7413–8238.

## Name operations (ns)

| Operation | `main` | series |
|---|---:|---:|
| `from_ascii`, 6 typical names | 3202 | 3242 |
| `==`, case variants, 6 names | 507 | 266 |
| `Hash`, 6 names | 456 | 153 |
| `to_lowercase`, 6 names | 219 | 178 |
| `clone`, 6 names | 71 | 40 |
| `cmp`, 6 names, case variants (includes ip6.arpa) | 509 | 604 |
| `iter().rev()`, 6 names | 139 | 671 |
| `iter().rev()`, 127 labels | 291 | 13013 |
| `iter()`, 127 labels | 208 | 263 |
| `parse_arpa_name`, ip6.arpa | 332 | 1077 |

## `cmp` by shape (ns)

| Shape | `main` | series |
|---|---:|---:|
| 3 labels, equal, case variants | 31 | 29 |
| 3 labels, differs at root-most label | 10 | 16 |
| 3 labels, differs at leaf label | 29 | 27 |
| 5 labels, equal | 43 | 37 |
| 5 labels, differs at root-most label | 10 | 21 |
| 8 labels, equal | 66 | 56 |
| 8 labels, differs at root-most label | 15 | 39 |
| 8 labels, differs at leaf label | 62 | 55 |
| 34 labels ip6.arpa, equal | 198 | 373 |
| 34 labels ip6.arpa, differs at root-most label | 10 | 260 |
| 34 labels ip6.arpa, differs at leaf label | 195 | 370 |
| `BTreeMap<LowerName>` lookup, 20k zone names | 730 | 690 |
| `HashMap<LowerName>` lookup, 20k zone names | 83 | 90 |

## Correctness checks

All on the merged series unless noted; identical outcomes on `main`
where the check is representation-independent.

- `cargo test -p hickory-proto --features dnssec-ring,serde`: 297 unit,
  2 integration, 48 doc tests pass.
- `cargo test --workspace --doc`: 53 pass.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- `cargo check -p hickory-proto --no-default-features`: clean.
- `cargo fmt --all -- --check`: one hunk in `crates/proto/benches/message.rs`
  from #1 (stable rustfmt 1.94).
- `cargo test --workspace --no-fail-fast`: 674 passed, 24 failed, 11
  ignored; `main` gives 673 / 24 / 11 with the identical failure list
  (IPv6 binds, `Address family not supported by protocol`).
- `review_props.rs`: 30k model pairs, 20k text round trips, 30k
  structured pointer cases, 3k messages re-encoded byte-identically, 400k
  garbage buffers without a panic.
- Decoder differential against a transcription of `main`'s state
  machine: 1.4M (buffer, offset) cases, identical labels, position, error
  variant and payload. Error classes: InsufficientBytes 431k,
  UnrecognizedLabelCode 457k, PointerNotPriorToLabel 363k,
  LabelOverlapsWithOther 13k, DomainNameTooLong 470; 135k successes.
- `TinyVec<[u8; N]>` sizes with tinyvec 1.11.0: 24 → 32, 32 → 40,
  40 → 48, 44 → 48, 46 → 56, 48 → 56, 64 → 72 bytes.

## Repository benches (nightly harness)

`RUSTFLAGS="--cfg=nightly" cargo +nightly bench -p hickory-proto --bench name
--bench message`, nightly 1.100.0 (2026-08-31), with the benches added by the
`name-shape-benches` branch applied to both trees. ns/iter, pass 1 / pass 2.
Raw output in `results/nightly-bench-*.log`.

| bench | `main` | series |
|---|---:|---:|
| `bench_parse_message` | 311 / 331 | 232 / 231 |
| `bench_parse_real_message` | 1149 / 1213 | 794 / 777 |
| `bench_parse_message_cname` | 455 / 451 | 324 / 316 |
| `bench_parse_message_referral` | 1171 / 1131 | 835 / 813 |
| `bench_emit_message` | 209 / 218 | 203 / 202 |
| `bench_emit_message_cve_2024_8508` | 374,765 / 370,066 | 320,343 / 341,504 |
| `bench_clone_message_cname` | 107 / 106 | 85 / 84 |
| `bench_clone_message_nxdomain` | 90 / 89 | 79 / 80 |
| `bench_clone_message_referral` | 292 / 281 | 203 / 199 |
| `name_cmp_short` / `medium` / `long` | 11.6 / 31.5 / 56.1 | 9.6 / 26.1 / 51.5 |
| `name_cmp_long_not_eq` | 15.0 / 14.9 | 19.3 / 19.7 |
| `name_cmp_long_not_eq_root` | 9.2 / 9.1 | 17.7 / 17.9 |
| `name_cmp_ip6_arpa` | 199 / 195 | 353 / 343 |
| `name_cmp_ip6_arpa_not_eq_root` | 12.7 / 12.8 | 240 / 238 |
| `name_eq_medium` | 29.2 / 27.9 | 3.8 / 3.9 |
| `name_hash_medium` | 40.8 / 40.4 | 18.8 / 18.6 |
| `name_to_lower_long` | 21.9 / 21.8 | 9.5 / 9.8 |
| `name_iter_rev_medium` | 7.0 / 7.0 | 7.3 / 7.3 |
| `name_iter_rev_ip6_arpa` | 57.5 / 55.1 | 557 / 565 |
| `name_iter_rev_max_labels` | 211 / 208 | 12,539 / 12,641 |
| `name_parse_arpa_name_ip6` | 358 / 360 | 1,091 / 1,040 |

# Round 2, 2026-09-01, after the fixes

Series heads: #1 `5b28474`, #2 `e13908a`, #3 `ddbe251` (includes #2), #4
`aa73353`, merged onto `main` together with #6 (`2dd3fc7`) and #7 (`316626d`).
The VM ran about 1.4× slower than in round 1, so only comparisons inside one
table are meaningful. Raw logs in `results/round2/`.

## Checks on the merged series

- rustfmt, clippy `--workspace --all-targets -- -D warnings`,
  `cargo check -p hickory-proto --no-default-features`: clean.
- `cargo test -p hickory-proto --features dnssec-ring,serde`: 311 unit,
  2 integration, 6 model (`name_props`), 48 doc tests pass;
  `HICKORY_NAME_PROPS_SCALE=10`: pass.
- `review_props.rs` (round-1 counts) including the decoder differential:
  pass, identical error-class counts to round 1.
- `cargo test --workspace --no-fail-fast`: 694 passed, 24 failed, 11 ignored;
  failure set identical to `main`.
- Sizes: `Name` 56, `LowerName` 56, `RData` 40, `Record` 104.

## Stable probes, one window: `main` | series before fixes | series after (pass 1 ; pass 2, ns)

| probe | `main` | before | after |
|---|---:|---:|---:|
| cmp equal, 3 labels | 51.1 ; 51.3 | 34.8 ; 36.1 | 29.3 ; 28.6 |
| cmp differs at root-most, 3 labels | 19.1 ; 18.0 | 21.1 ; 21.4 | 12.6 ; 12.5 |
| cmp differs at leaf, 3 labels | 36.8 ; 37.4 | 32.1 ; 32.9 | 39.1 ; 37.9 |
| cmp equal, 8 labels | 120.1 ; 122.5 | 65.7 ; 67.4 | 38.5 ; 38.5 |
| cmp differs at root-most, 8 labels | 21.3 ; 21.7 | 42.4 ; 44.0 | 16.8 ; 17.1 |
| cmp differs at leaf, 8 labels | 85.9 ; 89.7 | 63.0 ; 63.4 | 55.5 ; 56.6 |
| cmp equal, 34 labels ip6.arpa | 366.7 ; 355.2 | 376.1 ; 381.8 | 106.3 ; 106.3 |
| cmp differs at root-most, ip6.arpa | 16.3 ; 16.5 | 274.1 ; 268.3 | 13.2 ; 13.1 |
| cmp differs at leaf, ip6.arpa | 306.3 ; 321.6 | 390.6 ; 391.5 | 112.4 ; 111.1 |
| `BTreeMap<LowerName>` lookup, 20k (per 1000 lookups) | 962839 ; 991756 | 821111 ; 865480 | 840986 ; 806470 |
| `HashMap<LowerName>` lookup, 20k (per 1000 lookups) | 117726 ; 109770 | 96940 ; 96594 | 53461 ; 53535 |
| clone A answer (4 A) | 241.2 ; 240.3 | 211.9 ; 126.1 | 134.0 ; 129.6 |
| clone CNAME + A | 152.7 ; 151.4 | 111.6 ; 119.4 | 113.8 ; 113.1 |
| clone NXDOMAIN with SOA | 126.0 ; 126.8 | 102.6 ; 107.5 | 105.8 ; 106.0 |
| clone referral 4 NS + 4 A | 456.2 ; 460.0 | 258.7 ; 269.8 | 268.0 ; 273.5 |
| parse A answer, 97 B | 770.8 ; 769.2 | 445.2 ; 459.0 | 473.8 ; 447.6 |
| parse CNAME answer, 100 B | 602.1 ; 621.8 | 371.7 ; 385.4 | 397.8 ; 387.9 |
| parse NXDOMAIN, 85 B | 452.5 ; 456.5 | 348.5 ; 357.4 | 367.4 ; 366.6 |
| parse referral, 169 B | 1594.2 ; 1581.5 | 1029.0 ; 1064.4 | 1138.5 ; 1157.0 |
| parse 40 CNAMEs, 1243 B | 11336 ; 10925 | 9106 ; 9271 | 10102 ; 9753 |
| `Name::from_ascii`, 6 names | 3547 ; 3612 | 4087 ; 4056 | 4231 ; 4221 |
| `==` case variants, 6 names | 746.0 ; 745.3 | 286.3 ; 282.7 | 287.8 ; 284.1 |
| `cmp` case variants, 6 names | 622.4 ; 811.9 | 651.5 ; 633.9 | 323.3 ; 313.4 |
| hash, 6 names | 632.4 ; 675.3 | 188.9 ; 186.2 | 193.3 ; 200.8 |
| `to_lowercase`, 6 names | 258.2 ; 268.2 | 194.3 ; 194.6 | 199.9 ; 200.9 |
| `iter().rev()`, 6 names | 222.4 ; 247.5 | 869.9 ; 881.0 | 394.9 ; 375.2 |
| `parse_arpa_name`, ip6.arpa | 492.4 ; 518.2 | 1392.6 ; 1413.3 | 572.4 ; 568.2 |
| `iter().rev()`, 127 labels | 325.9 ; 330.1 | 14370 ; 14296 | 797.2 ; 780.3 |
| `iter()`, 127 labels | 337.7 ; 343.8 | 293.5 ; 290.5 | 299.8 ; 298.2 |
| `Name::clone`, 6 names | 90.7 ; 92.5 | 54.3 ; 55.6 | 55.9 ; 56.1 |

## Repository benches, one interleaved run: `main` vs series after (pass 1 / pass 2, ns/iter)

| bench | `main` | series |
|---|---:|---:|
| `bench_parse_message` | 334 / 338 | 264 / 260 |
| `bench_parse_real_message` | 1358 / 1344 | 957 / 951 |
| `bench_parse_message_cname` | 553 / 558 | 363 / 365 |
| `bench_parse_message_referral` | 1375 / 1421 | 1046 / 994 |
| `bench_emit_message` | 270 / 270 | 259 / 246 |
| `bench_clone_message_cname` | 141 / 138 | 120 / 120 |
| `bench_clone_message_nxdomain` | 122 / 124 | 106 / 103 |
| `bench_clone_message_referral` | 399 / 395 | 268 / 267 |
| `name_cmp_short` / `medium` / `long` | 17.6 / 45.9 / 81.6 | 14.1 / 29.3 / 20.4 |
| `name_cmp_long_not_eq` | 22.9 / 22.4 | 39.3 / 39.1 |
| `name_cmp_medium_not_eq` | 21.7 / 22.8 | 26.4 / 26.5 |
| `name_cmp_long_not_eq_root` | 14.0 / 14.6 | 8.1 / 7.8 |
| `name_cmp_ip6_arpa` | 353 / 357 | 28.6 / 27.9 |
| `name_cmp_ip6_arpa_not_eq_root` | 19.7 / 20.1 | 10.8 / 10.9 |
| `name_eq_medium` | 43.4 / 43.6 | 7.1 / 6.9 |
| `name_hash_medium` | 66.7 / 66.7 | 19.7 / 19.8 |
| `name_to_lower_long` | 27.1 / 27.5 | 12.8 / 12.3 |
| `name_iter_rev_medium` | 8.6 / 8.6 | 19.3 / 19.0 |
| `name_iter_rev_ip6_arpa` | 66.5 / 66.1 | 212 / 211 |
| `name_iter_rev_max_labels` | 219 / 212 | 808 / 803 |
| `name_parse_arpa_name_ip6` | 584 / 603 | 606 / 609 |

# Round 3, 2026-09-02, after the simplifications (second brief, phase 1)

Series heads: #6 `5ec9fc6`, #7 `39e0983`, #1 `5b28474`, #2 `c03fada`, #3 `31ac27a`, #4 `aa73353`.
Columns: `main` (with #7's benches) | round-2 heads (#2 `e13908a`, #3 `ddbe251`) | round-3 heads, pass 1 ; pass 2.
Checks on the merged series: rustfmt, clippy, no-std, proto suite (311 unit, 5 model, 48 doc), decoder differential identical, workspace 693 passed with `main`'s failure set, sizes 56/56/40/104. Raw logs in `results/round3/`.

## Repository benches, nightly, one interleaved run (ns/iter)

| bench | `main` | round 2 | round 3 |
|---|---:|---:|---:|
| `bench_clone_message_cname` | 144.17 ; 148.44 | 127.59 ; 114.20 | 112.89 ; 112.31 |
| `bench_clone_message_nxdomain` | 125.05 ; 128.08 | 125.45 ; 103.64 | 123.67 ; 115.55 |
| `bench_clone_message_referral` | 391.00 ; 401.61 | 272.36 ; 258.65 | 255.12 ; 265.92 |
| `bench_emit_header` | 64.52 ; 64.24 | 62.12 ; 62.06 | 60.52 ; 68.38 |
| `bench_emit_message` | 265.94 ; 266.55 | 255.31 ; 256.33 | 246.47 ; 292.85 |
| `bench_emit_message_cve_2024_8508` | 470,014.38 ; 481,504.81 | 429,669.51 ; 400,015.28 | 410,121.03 ; 471,646.14 |
| `bench_emit_message_no_reservation` | 282.47 ; 277.39 | 253.69 ; 260.58 | 249.85 ; 291.42 |
| `bench_parse_header` | 4.77 ; 4.85 | 4.64 ; 4.52 | 4.78 ; 5.36 |
| `bench_parse_header_no_reservation` | 75.16 ; 71.33 | 66.82 ; 67.24 | 65.83 ; 76.42 |
| `bench_parse_message` | 331.31 ; 341.52 | 263.77 ; 261.22 | 265.88 ; 290.37 |
| `bench_parse_message_referral` | 1,358.40 ; 1,436.95 | 1,009.72 ; 988.71 | 955.13 ; 1,066.54 |
| `bench_parse_real_message` | 1,297.69 ; 1,477.14 | 961.89 ; 970.57 | 907.56 ; 1,016.81 |
| `name_cmp_ip6_arpa` | 352.18 ; 355.74 | 27.62 ; 27.26 | 14.58 ; 14.39 |
| `name_cmp_ip6_arpa_not_eq_root` | 20.23 ; 20.12 | 10.89 ; 10.74 | 170.48 ; 177.58 |
| `name_cmp_long` | 84.17 ; 84.30 | 20.17 ; 20.63 | 11.02 ; 11.04 |
| `name_cmp_long_case` | 66.08 ; 66.78 | 14.15 ; 14.07 | 5.89 ; 5.77 |
| `name_cmp_long_not_eq` | 22.83 ; 22.77 | 38.33 ; 39.31 | 32.00 ; 30.07 |
| `name_cmp_long_not_eq_root` | 14.07 ; 14.64 | 8.00 ; 7.82 | 63.42 ; 64.45 |
| `name_cmp_medium` | 45.83 ; 48.04 | 27.80 ; 27.72 | 9.83 ; 9.80 |
| `name_cmp_medium_case` | 36.83 ; 36.42 | 14.25 ; 14.48 | 5.70 ; 5.86 |
| `name_cmp_medium_not_eq` | 22.92 ; 23.29 | 27.03 ; 26.87 | 17.46 ; 16.74 |
| `name_cmp_short` | 17.69 ; 17.94 | 14.21 ; 14.02 | 10.99 ; 10.84 |
| `name_cmp_short_case` | 14.48 ; 14.83 | 13.49 ; 13.76 | 5.61 ; 5.94 |
| `name_cmp_short_not_eq` | 12.04 ; 12.27 | 8.86 ; 9.16 | 13.18 ; 12.97 |
| `name_eq_medium` | 42.73 ; 44.75 | 6.86 ; 8.01 | 5.94 ; 5.98 |
| `name_hash_medium` | 68.70 ; 66.27 | 19.79 ; 22.59 | 19.80 ; 19.61 |
| `name_iter_rev_ip6_arpa` | 66.59 ; 68.27 | 222.06 ; 247.81 | 151.12 ; 152.79 |
| `name_iter_rev_max_labels` | 210.05 ; 209.86 | 839.05 ; 882.69 | 509.66 ; 504.98 |
| `name_iter_rev_medium` | 8.75 ; 8.63 | 19.86 ; 22.05 | 11.19 ; 11.36 |
| `name_no_lower_long` | 28.30 ; 27.99 | 12.79 ; 13.51 | 12.26 ; 12.25 |
| `name_no_lower_medium` | 26.63 ; 25.25 | 22.56 ; 25.38 | 20.57 ; 20.45 |
| `name_no_lower_short` | 19.15 ; 18.81 | 13.40 ; 14.84 | 13.30 ; 12.92 |
| `name_parse_arpa_name_ip6` | 623.97 ; 613.20 | 611.53 ; 724.94 | 660.63 ; 572.45 |
| `name_to_lower_long` | 27.87 ; 28.64 | 12.96 ; 13.78 | 13.80 ; 12.26 |
| `name_to_lower_medium` | 25.68 ; 25.67 | 23.48 ; 26.98 | 22.54 ; 20.63 |
| `name_to_lower_short` | 18.84 ; 18.35 | 13.31 ; 15.12 | 13.93 ; 13.23 |

## Stable probes, one window (ns)

| probe | `main` | round 2 | round 3 |
|---|---:|---:|---:|
| cmp equal, case-variant, 3 labels | 51.0 ; 49.3 | 28.8 ; 29.7 | 25.6 ; 26.8 |
| cmp differs at root-most label, 3 labels | 17.9 ; 17.4 | 12.2 ; 12.1 | 54.3 ; 56.9 |
| cmp differs at leaf label, 3 labels | 37.7 ; 36.8 | 38.4 ; 39.9 | 31.2 ; 32.1 |
| cmp equal, case-variant, 5 labels | 74.6 ; 72.1 | 30.9 ; 30.7 | 28.3 ; 29.5 |
| cmp differs at root-most label, 5 labels | 17.9 ; 18.0 | 11.9 ; 11.9 | 62.3 ; 66.2 |
| cmp differs at leaf label, 5 labels | 54.9 ; 53.5 | 43.8 ; 41.7 | 35.6 ; 36.7 |
| cmp equal, case-variant, 8 labels | 123.0 ; 116.5 | 38.2 ; 37.7 | 35.7 ; 35.6 |
| cmp differs at root-most label, 8 labels | 22.0 ; 21.0 | 16.6 ; 16.3 | 87.1 ; 90.7 |
| cmp differs at leaf label, 8 labels | 85.8 ; 84.8 | 54.4 ; 54.8 | 43.2 ; 42.9 |
| cmp equal, case-variant, 34 labels ip6.arpa | 359.6 ; 357.3 | 104.4 ; 107.7 | 106.8 ; 106.2 |
| cmp differs at root-most label, 34 labels ip6.arpa | 16.2 ; 16.1 | 13.1 ; 13.0 | 275.0 ; 272.2 |
| cmp differs at leaf label, 34 labels ip6.arpa | 303.8 ; 304.1 | 109.8 ; 109.7 | 114.4 ; 114.9 |
| BTreeMap<LowerName> lookup, 20k entries (per lookup) | 991206.2 ; 938273.2 | 763848.7 ; 775584.6 | 673570.0 ; 653412.2 |
| HashMap<LowerName> lookup, 20k entries (per lookup) | 107921.6 ; 104460.0 | 52480.3 ; 54415.7 | 54522.8 ; 52207.7 |
| clone A answer (4 A) | 249.2 ; 235.6 | 131.3 ; 129.7 | 130.3 ; 135.9 |
| clone CNAME answer (CNAME + A) | 150.3 ; 144.2 | 114.4 ; 115.3 | 113.1 ; 132.2 |
| clone NXDOMAIN (SOA in authority) | 127.1 ; 122.9 | 105.9 ; 105.6 | 104.0 ; 106.7 |
| clone referral (4 NS + 4 A) | 451.7 ; 440.9 | 268.5 ; 263.6 | 262.6 ; 259.0 |
| parse A answer (97 bytes) | 780.4 ; 776.2 | 467.6 ; 453.4 | 441.1 ; 443.3 |
| emit A answer | 877.8 ; 888.6 | 818.5 ; 826.1 | 753.3 ; 747.2 |
| parse CNAME answer (100 bytes) | 590.1 ; 605.1 | 391.2 ; 391.5 | 407.1 ; 377.5 |
| emit CNAME answer | 1165.0 ; 1180.7 | 1090.1 ; 1106.6 | 1047.3 ; 1037.0 |
| parse NXDOMAIN (85 bytes) | 445.7 ; 454.6 | 369.3 ; 364.1 | 371.2 ; 359.0 |
| emit NXDOMAIN | 774.8 ; 783.0 | 783.0 ; 751.6 | 713.6 ; 715.2 |
| parse referral (169 bytes) | 1580.4 ; 1570.5 | 1119.6 ; 1127.9 | 1106.9 ; 1049.3 |
| emit referral | 2121.3 ; 2148.0 | 1965.6 ; 2043.6 | 1867.6 ; 1826.8 |
| parse 40-CNAME response (1243 bytes) | 10662.7 ; 10707.6 | 9493.4 ; 9789.6 | 9551.0 ; 9492.7 |
| Name | 3714.1 ; 3435.4 | 4218.6 ; 4381.7 | 4086.8 ; 4052.2 |
| Name eq case-variant (6 names) | 760.4 ; 738.3 | 283.8 ; 287.1 | 284.9 ; 285.4 |
| Name cmp case-variant (6 names) | 641.1 ; 635.4 | 312.4 ; 311.5 | 290.5 ; 296.8 |
| Name hash (6 names) | 667.0 ; 649.9 | 186.7 ; 189.9 | 188.5 ; 190.5 |
| Name to_lowercase (6 names) | 261.1 ; 260.7 | 194.1 ; 197.3 | 196.1 ; 197.2 |
| iter().rev() full walk (6 names) | 237.9 ; 233.3 | 373.5 ; 380.3 | 260.6 ; 266.7 |
| parse_arpa_name ip6.arpa (34 labels) | 500.3 ; 491.8 | 574.4 ; 577.0 | 517.8 ; 519.5 |
| iter().rev() full walk, 127 labels | 323.7 ; 333.7 | 782.5 ; 801.3 | 575.6 ; 576.5 |
| iter() full walk, 127 labels | 319.2 ; 318.6 | 297.8 ; 299.1 | 297.5 ; 298.8 |
| Name clone (6 names) | 88.2 ; 88.3 | 56.0 ; 55.9 | 53.0 ; 53.9 |
