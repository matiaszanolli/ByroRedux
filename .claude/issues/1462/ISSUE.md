# REN-DIM18-01

**Issue:** #1462
**Filed:** 2026-06-04
**Source report:** docs/audits/AUDIT_RENDERER_2026-06-04_DIM18.md

---

**Severity:** LOW (latent — harmless while `VOLUMETRIC_OUTPUT_CONSUMED == false`; manifests only when the const flips for M-LIGHT v2)
**Dimension:** Volumetrics (M55)
**Source report:** `docs/audits/AUDIT_RENDERER_2026-06-04_DIM18.md`
**Location:** `crates/renderer/shaders/volumetrics_inject.comp:100-104`, `crates/renderer/shaders/volumetrics_integrate.comp:55-71`, `crates/renderer/shaders/composite.frag:396-397`

## Description
The froxel pipeline uses three slightly different depth conventions:
- **Inject** samples each froxel at its **center**: `uvw.z = (coord.z + 0.5) / size.z`, `t = uvw.z * VOLUME_FAR` (`volumetrics_inject.comp:100,104`).
- **Integrate** accumulates a **front-of-slab** Riemann sum: `inscatter_total += inscatter * trans_cumulative * dt;` is applied *before* `trans_cumulative *= exp(-extinction * dt);` (`volumetrics_integrate.comp:64-65`), so slice `i`'s inscatter is weighted by transmittance through slices `0..i-1` (front edge), not the slice midpoint.
- **Composite** reconstructs the slice by **texel-edge** sample: `slice = clamp(worldDist / VOLUME_FAR, 0, 0.9999)` with CLAMP_TO_EDGE + bilinear (`composite.frag:396-397`).

Under the documented Phase-2 linear distribution this is a ~half-slab (~0.78 m at 200 m / 128 slices) bias in the transmittance/inscatter curve.

## Impact
Sub-froxel fog-depth bias when the output is consumed; no crash, no NaN. **Harmless today** because `VOLUMETRIC_OUTPUT_CONSUMED == false` (`volumetrics.rs:124`) and the composite read is dead. Becomes a subtle fog-depth offset the moment the const flips.

## Suggested Fix
When M-LIGHT v2 flips the const, reconcile the three conventions — shift composite's `slice` mapping by +0.5 texel to match the inject center sample, or weight the integrate inscatter by the midpoint transmittance `sqrt(exp(-extinction*dt))`. Alternatively, document the half-slab bias as accepted under the Phase-2 linear model. Fold into the same changeset that flips `VOLUMETRIC_OUTPUT_CONSUMED`.

## Completeness Checks
- [ ] **SIBLING**: all three depth conventions (inject / integrate / composite) reconciled in lockstep, not just one
- [ ] **TESTS**: if a host-side froxel-depth mapping helper is added, pin inject↔composite slice agreement with a unit test
- [ ] Tracked on the `VOLUMETRIC_OUTPUT_CONSUMED` flip checklist (#928) so it cannot be flipped without addressing this

_No action required now — latent until the const flips._
