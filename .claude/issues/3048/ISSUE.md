# SAFE-2026-08-16-01: FaceGen morph NaN guard checks inputs, not outputs

**Issue**: #3048
**Severity**: MEDIUM
**Labels**: `medium,safety,bug`
**Source report**: `docs/audits/AUDIT_SAFETY_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_SAFETY_2026-08-16.md`.

**Location**: `crates/facegen/src/eval.rs`:62-100 (guard chain at :74, :79, :93; the unchecked product at :82 and unchecked accumulation at :96-98) · live call site `byroredux/src/npc_spawn/resumable.rs`:971-973

## Description

The FaceGen morph evaluator's NaN guard checks **every input and no output**. `finite × finite` can overflow to `±inf`, and nothing re-checks after the multiply or the accumulation — so `±inf` vertex positions reach the vertex SSBO and the BLAS build.

## Evidence

```rust
// crates/facegen/src/eval.rs (re-verified 2026-08-17)
if !scale.is_finite() { … }              // :74  input checked
let coeff = w * scale;                   // :82  product NOT checked
…
if !d[0].is_finite() || !d[1].is_finite() || !d[2].is_finite() { … }   // :93 input checked
out[i][0] += coeff * d[0];               // :96  accumulation NOT checked
out[i][1] += coeff * d[1];
out[i][2] += coeff * d[2];
```

Both guarded values are inputs; neither the product nor the running sum is validated.

## Impact

`.tri`/`.egt` morph data is **untrusted archive input** (`crates/facegen` is on `_audit-common.md`'s un-owned-subsystem list). A crafted or corrupt morph set with large coefficients produces `±inf` vertex positions, which flow into the vertex SSBO and then into `build_blas_for_mesh`.

Non-finite geometry in an acceleration-structure build is undefined behaviour at the Vulkan level — the spec requires finite vertex data — so this is a driver-dependent failure, not a clean error.

## Suggested Fix

Check the accumulated output for finiteness before writing it out (once per vertex, after the accumulation loop, not per-term), and reject or clamp the morph on failure. Guarding the output is strictly stronger than guarding inputs here and costs one check per vertex.

## Related

- `crates/facegen` has no owner audit skill — see `_audit-common.md`'s un-owned-subsystem table
- #3011 (SCR-D8-01) — the same untrusted-parser-input class in `crates/hkx`

## Completeness Checks
- [ ] **UNSAFE**: N/A — no `unsafe` involved; the hazard is non-finite data reaching the GPU
- [ ] **OUTPUT-CHECKED**: The guard covers the accumulated result, not only the inputs
- [ ] **SIBLING**: The `.egt` texture-blend path checked for the same input-only guarding
- [ ] **BLAS-SAFETY**: Non-finite vertices cannot reach `build_blas_for_mesh`
- [ ] **TESTS**: A negative-input test feeds overflow-inducing coefficients and asserts rejection

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3048 --json state` when live state is needed.*
