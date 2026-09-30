# #4837: REN-D8-2026-09-24-03: `#4285`'s WATAL normalization divides Starfield's `oceanness` lane by 20 as well, weakening it ~20× on both shader consumers (regression of #4285)

**Labels**: bug,medium,game:starfield,water

_Filed from `docs/audits/AUDIT_RENDERER_2026-09-24.md` (full renderer audit, audited `main` @ `6c5555c70`; delta baseline `f97775ca8`). Finding ID: **REN-D8-2026-09-24-03**._

**Regression of #4285** (closed): the fix landed but the defect returns — see Status below.

- **Severity**: MEDIUM. Wrong translated value on one game's water, visual only (Starfield oceans). Not literally the NIFAL `translate_material` boundary, but the same one-boundary logic applies.
- **Dimension**: Water
- **Location**: `byroredux/src/env_translate.rs` — `resolve_water_noise_and_rain` (the `mat.concentration` loop); consumers `crates/renderer/shaders/water.frag` `absorbWaterColumn` (`+ clamp(push.concentration.a, 0.0, 1.0) * 0.25`) and the `oceanScatter = 1.0 + clamp(push.concentration.a, 0.0, 1.0) * 0.5` line; doc `crates/core/src/ecs/components/water.rs` `WaterMaterial.concentration`.
- **Status**: NEW. **Regression introduced by `6352dba17`** (Fix #4285, 2026-09-23). Related closed #3227. The orchestrator re-read the loop.
- **Description**: The commit moved the per-game unit constant out of `water.frag`; the old shader divided only `push.concentration.rgb` by `STARFIELD_WATER_CONCENTRATION_REFERENCE` (20.0) and read `.a` raw. The new boundary loop is `for (dst, src) in mat.concentration.iter_mut().zip(rec.params.concentration) { if src.is_finite() && src > 0.0 { *dst = (src / STARFIELD_WATER_CONCENTRATION_REFERENCE).clamp(0.0, 1.0); } }`. It runs over **all four** lanes. Its own comment ("the division is a no-op for it in practice"), the commit body ("the natively-0..1 oceanness lane passes through") and the `WaterMaterial.concentration` doc all say the opposite of what the loop does.
- **Evidence**: The updated test asserts `[8.840 / 20.0, 6.594 / 20.0, 4.710 / 20.0, 0.514 / 20.0]`, so it pins the bug. #3227 / the 2026-08-20 Starfield audit measured oceanness authored in 0 / 0.514 / 0.699 / 1.0 across the vanilla records (one test record 1.63), i.e. natively 0..1. The forward-scatter boost of open-ocean water is at most +2.5 % instead of +50 %, and the density contribution at most 0.0125 instead of 0.25.
- **Impact**: Starfield oceans lose the authored oceanness response in both the Beer-Lambert density and the sun-forward scattering. Not visible to any test.
- **Suggested Fix**: Normalize only lanes 0..2 (`.take(3)`), pass lane 3 through with its own clamp, and change the test's fourth expected value to `0.514`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders, other producers/consumers, other games)
- [ ] **TESTS**: A regression test pins this specific fix (and fails when the guarded code is deleted — mutation-check it)

