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
