# Issue #875 (OPEN): NIF-PERF-11: morph.rs re-allocates Vec<NiPoint3> to Vec<[f32; 3]> despite identical layout

URL: https://github.com/matiaszanolli/ByroRedux/issues/875

---

## Description

`crates/nif/src/blocks/controller/morph.rs:221-222` reads `num_vertices` `NiPoint3` values via the optimal `read_ni_point3_array` bulk reader, then immediately throws away the result and re-allocates a fresh `Vec<[f32; 3]>` with bitwise-identical content.

`NiPoint3` is `#[repr(C)]` with three `f32` fields and no padding (`crates/nif/src/types.rs:15-21`). Layout is bitwise identical to `[f32; 3]`. The `collect` is a no-op axis swap.

## Evidence

```rust
// morph.rs:221-222
let points = stream.read_ni_point3_array(num_vertices as usize)?;
let vectors: Vec<[f32; 3]> = points.into_iter().map(|p| [p.x, p.y, p.z]).collect();
```

## Why it matters

Each FaceGen head with 8 morph targets × 5,000 vertices is ~40 KB of redundant copy. Across a Whiterun load with ~50 NPC heads, ~2 MB of throwaway memcpy on a path that's already on the cell-load critical path.

## Proposed Fix

Option A (preferred — smallest blast radius): change `MorphTarget.vectors` from `Vec<[f32; 3]>` to `Vec<NiPoint3>` so the read result is consumed in place. Downstream consumers receive `&NiPoint3` and read `.x / .y / .z` (already the typical pattern in `import/mesh.rs`).

Option B (zero-copy alternative): keep the field type, add `read_f32_triple_array` helper backed by `read_pod_vec::<[f32; 3]>(count)` — same trick as #874 (NIF-PERF-10). Bigger blast radius for a smaller win.

## Cost Estimate

Per-morph-target × `num_vertices`; only triggers on FaceGen-bearing NIFs. Cell-load critical path on cells with many NPCs.

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Audit other `Vec<NiPoint3>` → `Vec<[f32; 3]>` collect-conversions in the parser
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Existing morph-target integration test must produce bit-identical vertex deltas

## dhat Gap

Expected savings are estimates; no quantitative regression guard exists today. This finding warrants a follow-up "wire dhat for morph parse" issue.

## References

- Audit: `docs/audits/AUDIT_PERFORMANCE_2026-05-06.md` (NIF-PERF-11)
