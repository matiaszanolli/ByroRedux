# NIF-D6-INFO-03: dhat-infra gap — no allocation-counter regression test for NIF-PERF-01/02/03 or #408

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/1247

**Source**: `docs/audits/AUDIT_NIF_2026-05-23.md` (Dim 6, INFO finding re-flagged from earlier audits)
**Severity**: LOW (visibility flag, not a defect — audit-skill INFO promoted to LOW for tracking)
**Dimension**: Test Infrastructure

## Description

All four architectural pins (NIF-PERF-01 #832 / NIF-PERF-02 #833 / NIF-PERF-03 #831 / #408 blanket sweep) shipped as code-review-only fixes. There is no test that, given a fixed corpus NIF, asserts an upper bound on heap allocations (count or bytes).

A regression that re-introduces e.g. the `or_insert(name.to_string())` pattern would compile, pass every existing test, and only manifest as a multi-megabyte `htop`-visible RSS bump on long Oblivion cell walks. The fixes themselves are mature, but their **durability across future block-parser additions depends entirely on grep-based audits**.

Verification:
```
$ grep dhat Cargo.toml crates/*/Cargo.toml
(no hits)
$ grep -rn "dhat\|alloc_counter\|GlobalAlloc" crates/nif/
(no hits)
```

## Impact

Audit gap. Confidence in the pins comes from manual sweeps, not from CI. Every NIF parser audit (this one included) has to re-run the grep checks for the 4 pins; a CI gate would shift that burden once.

## Suggested Fix

Add `dhat = "0.3"` as an optional workspace dep gated on a `dhat-heap` feature; wire a `#[cfg(feature = "dhat-heap")]` test that parses a representative NIF and asserts:

```rust
assert!(dhat::HeapStats::total_blocks() < N);
assert!(dhat::HeapStats::total_bytes() < M);
```

Candidate fixtures:
- `meshes/architecture/megaton/megaton01.nif` for the LRU-counter path
- Any `meshes/skeleton.nif` for the bulk-array path

Pick `N` and `M` empirically at first; tighten over time as fixes land. CI runs `cargo test --features dhat-heap` as a sibling job.

## Related

- #832 (CLOSED, NIF-PERF-01): per-block parse-loop counter — `entry().get_mut() / insert` split
- #833 (CLOSED, NIF-PERF-02): bulk-array readers via `read_pod_vec`
- #831 (CLOSED, NIF-PERF-03): `#[must_use]` on `allocate_vec`
- #408 (CLOSED): blanket `allocate_vec` sweep
- #1246 (this audit): `#[must_use]` on `read_pod_vec` wrappers — same defense-in-depth shape, code-review-only without this CI gate

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: once the CI gate exists, audit the matching renderer / ECS / ESM hot paths for similar allocation-counter coverage gaps. The dhat feature can be reused across crates.
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: this issue IS the test addition — closes when `dhat-heap` feature lands + at least one allocation-bound test exists in `crates/nif/tests/`