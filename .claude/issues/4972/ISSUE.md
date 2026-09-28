# #4972: REN-D10-2026-09-27-03: Residue of closed #4860 — `spawn_nif_lights` fabricates `SHADOW_OMNIDIRECTIONAL` into both flag words, and `Emitter::default()` still carries `ARCHITECTURE`

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4972
- **Labels**: low,renderer,legacy-compat,bug

_Filed from `docs/audits/AUDIT_RENDERER_2026-09-27.md` (full renderer audit, all 12 dimensions, audited `main` @ `7e9da5dcc`; code at the finding's location unchanged at `fe80f4d76`). Finding ID: **REN-D10-2026-09-27-03**._

- **Severity**: LOW
- **Dimension**: Light Animation
- **Location**: `byroredux/src/cell_loader/spawn.rs` `spawn_nif_lights` (the `LightSource::from_legacy_world_units(... LIGHT_FLAG_SHADOW_OMNIDIRECTIONAL, ... LIGHT_FLAG_SHADOW_OMNIDIRECTIONAL)` call, comment at L1139–1140); `crates/core/src/lighting.rs` `impl Default for Emitter` (`visibility: VisibilityMask::ARCHITECTURE`, L243), inherited by `LightSource::default()`.
- **Status**: Residual of #4860 (CLOSED). The ESM FO3/FNV synthesis that #4860 named was removed in `ab255cfd2`. The NIF-direct twin and the `Emitter::default().visibility` item that #4860 also listed were not.
- **Description**:
  - A direct `NiLight` has no LIGH flags, yet it is stored with `flags = 0x1000` and `shadow_flags = 0x1000`. The comment ("Preserve its authored physical visibility explicitly at this boundary") has been false since `b9e961eeb`, because `for_legacy_local_light()` now ignores flags. The `light` console command (`commands/scene.rs`) prints `legacy_flags=0x00001000 shadow_flags=0x00001000` as if the lamp had authored them. That is the "fabricated diagnostic" class that `fallout3nv_zero_projection_flags_remain_zero_diagnostics` now forbids on the ESM side.
  - `Emitter::default()` (and therefore `LightSource::default()`) is the one construction path whose visibility is not `FULL`. It has no production caller today, which makes it a latent trap for the next procedural producer.
  - The directional light (`render/lights.rs` `params.z`) and `volumetrics.rs` `combustion_light_from_moment` both write `FULL` literally, which is consistent with the single policy.
- **Impact**: Diagnostics are wrong for every in-mesh NIF light. No rendering effect today.
- **Suggested Fix**: Pass `0, …, 0` for the NIF-direct flags (or a named "derived" constant that the console labels as such), and delete the stale comment. Make `Emitter::default().visibility` `VisibilityMask::for_legacy_local_light()` (`FULL`).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders, other producers/consumers, other games)
- [ ] **TESTS**: A regression test pins this specific fix (and fails when the guarded code is deleted — mutation-check it)
