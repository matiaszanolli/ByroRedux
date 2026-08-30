# #3798: REG-2026-08-30-08: both manual_bench_draw_sort_* benches panic on integer overflow in a debug build — (i * 2654435761) needs wrapping_mul

**Labels**: bug, renderer, low, tech-debt
**Filed**: 2026-08-30 · HEAD `64f64480`

---

**Source**: `docs/audits/AUDIT_REGRESSION_2026-08-30.md` — REG-2026-08-30-08 (LOW)
**Dimension**: Test hygiene
**Location**: `byroredux/src/render/draw_sort_key_tests.rs:505` and `:602` (both `manual_bench_draw_sort_*` benches)

## Description

Both benches compute:

```rust
c.mesh_handle = (i as u32 * 2654435761) & 0xFFFF;
```

`2654435761 > u32::MAX / 2`, so the plain `*` overflows for `i >= 2` and **panics under `debug_assertions`**.

Verified at HEAD (`64f64480`): the literal appears at exactly those two sites and is still a plain `*`.

## Evidence

Verified **statically**. The benches were deliberately **not** run: `--ignored` is forbidden in this session (the ignored ESM-parsing tests are the confirmed OOM culprit).

Both benches are `#[ignore]` and documented `--release`, so this never reaches CI.

## Impact

No production defect and no CI exposure. The failure mode is a maintainer running `cargo test … -- --ignored` **without** `--release` and getting an integer-overflow panic instead of a measurement — an ignored test that fails for a reason unrelated to what it measures.

## Suggested Fix

`wrapping_mul(2654435761)` at both sites, matching the `wrapping_add` already used two lines below for `sort_depth`. One-line change each; no behaviour change in release.

## Completeness Checks
- [ ] **SIBLING**: Same `* 2654435761` hash-mix pattern checked across the other `manual_bench_*` sites in the tree
- [ ] **TESTS**: N/A — the fix is to the bench harness itself; correctness is that it no longer panics in a debug build
