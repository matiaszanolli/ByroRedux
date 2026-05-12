# Issue #981

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/981
**Title**: NIF-D6-NEW-05: read_pod_vec migration incomplete — ~14 POD-scalar sites still pay double-allocation
**Labels**: enhancement, nif-parser, medium, performance
**Audit source**: docs/audits/AUDIT_NIF_2026-05-12.md

---

**Source**: `docs/audits/AUDIT_NIF_2026-05-12.md` (Dim 6)
**Severity**: MEDIUM
**Dimension**: Allocation Hygiene
**Game Affected**: FO4 / FO76 / Skyrim animation-heavy + collision-heavy content (B-spline / packed-tristrip / DecalVector loads)

## Description

The `#833` rewrite collapsed `stream.rs`'s bulk-array readers via `read_pod_vec`, but ~14 sibling call sites in `crates/nif/src/blocks/` still use the `allocate_vec + per-element-push` shape. POD-typed, fixed stride, no per-element transform — exactly the helper's use case.

The prior audit's NIF-D6-NEW-04 finding ("remaining `Vec::with_capacity` hits are derived from already-validated counts") claimed coverage was complete. It wasn't — the migration was applied to `stream.rs`'s wrappers but not propagated to call sites that already had their own `allocate_vec + per-element loop` shape.

## Sites (verified 2026-05-12)

| File | Lines | Field | Type | n source |
|---|---|---|---|---|
| `blocks/interpolator.rs` | 1170-1174 | `NiBSplineData.float_control_points` | `Vec<f32>` | file u32 |
| `blocks/interpolator.rs` | 1175-1181 | `NiBSplineData.compact_control_points` | `Vec<i16>` (u16 raw) | file u32 |
| `blocks/extra_data.rs` | 147-153 | `NiIntegersExtraData.integers_array` | `Vec<u32>` | file u32 |
| `blocks/extra_data.rs` | 159-165 | `NiFloatsExtraData.floats_array` | `Vec<f32>` | file u32 |
| `blocks/extra_data.rs` | 470-474 | `BsEyeCenterExtraData.floats` | `Vec<f32>` | file u32 |
| `blocks/extra_data.rs` | 519-525 | `DecalVectorBlock.points` | `Vec<[f32; 3]>` | file u32 |
| `blocks/extra_data.rs` | 527-532 | `DecalVectorBlock.normals` | `Vec<[f32; 3]>` | file u32 |
| `blocks/collision.rs` | 1273-1278 | `bhkPackedNiTriStripsData.indices` | `Vec<u16>` | file u32 |
| `blocks/collision.rs` | 1280-1284 | `bhkPackedNiTriStripsData.strips` | `Vec<u16>` | file u32 |
| `blocks/tri_shape.rs` | 1133-1145 | segment offsets | `Vec<u32>` | file u32 |
| `blocks/tri_shape.rs` | 1150-1156 | cut offsets | `Vec<f32>` | file u32 |
| `blocks/legacy_particle.rs` | 473-477 | `emitter_points` | `Vec<u32>` | file u32 |
| `blocks/legacy_particle.rs` | 675-678, 687-690, 716-719 | particle radii / sizes / rotation_angles | `Vec<f32>` | file u32 |
| `blocks/shader.rs` | 451-454, 455-458 | `sf1_crcs` / `sf2_crcs` | `Vec<u32>` | file u32 |

## Why MEDIUM not LOW

This is **not** the original NIF-PERF-02 regression pattern (that was the *unbounded* `Vec::with_capacity` → fill). All sites above are gated by `allocate_vec`, so the byte-budget guard applies. What's left is residual double-allocation / per-element-read overhead — `allocate_vec` returns a `Vec<T>` with capacity but length 0, then the loop pushes `count` times. `read_pod_vec` collapses both into one `vec![T::default(); n]` + one `read_exact`.

Per-block savings: one capacity-vs-length divergence + `n` redundant bound checks. Aggregate on `NiBSplineData`-heavy FO4 animation content (thousands of control points per clip × dozens of clips × cell) is observable.

The MEDIUM tier also reflects that prior NIF-D6-NEW-04 explicitly claimed "coverage complete." Re-classifying this as LOW would let the same false-completeness claim recur.

## Suggested Fix

Mechanical sweep — replace each site with the appropriate bulk reader:

```rust
// Before:
let mut float_control_points: Vec<f32> = stream.allocate_vec(num_float)?;
for _ in 0..num_float {
    float_control_points.push(stream.read_f32_le()?);
}

// After:
let float_control_points = stream.read_f32_array(num_float as usize)?;
```

For `Vec<[f32; 3]>` decal sites: either use `read_ni_point3_array` and map, or add a new `read_f32_triple_array` wrapper on the same pattern as the existing `read_u16_triple_array` (`stream.rs:361`).

## dhat-infra gap (known)

No `dhat` / `GlobalAlloc` shim / allocation-counter test exists today. The architectural pins are defended by `#[must_use]`, byte-budget guards, and code review only. **This fix should land alongside (or behind) an allocation-counter regression test** — otherwise the next migration-completeness claim will re-recur. Filing the dhat-infra setup as a separate prerequisite is reasonable.

## Completeness Checks

- [ ] **MECHANICAL_SWEEP**: All 14 sites in the table above converted; CI is the test
- [ ] **NEW_HELPER**: If `read_f32_triple_array` is added, confirm it matches the LE-host gate pattern + zero bytemuck dep
- [ ] **REGRESSION_HARNESS**: `cargo test -p byroredux-nif` + `nif_stats --archive` byte-for-byte equivalence on a 10K-file FO4 BSA before/after
- [ ] **DHAT_INFRA**: File or link the prerequisite allocation-counter regression test (otherwise this won't stay fixed)
- [ ] **SIBLING_SWEEP**: After the 14 listed sites, grep for any remaining `allocate_vec.*?\n.*?for.*?\n.*?push.*?` patterns in `blocks/` — there may be sites the audit missed

## Audit reference

`docs/audits/AUDIT_NIF_2026-05-12.md` § Findings → MEDIUM → NIF-D6-NEW-05.

Related: #831 NIF-PERF-03 (`#[must_use]` on allocate_vec), #832 NIF-PERF-01 (counter `get_mut`/`insert`), #833 NIF-PERF-02 (read_pod_vec helper — this finding's parent — claimed migration complete).

