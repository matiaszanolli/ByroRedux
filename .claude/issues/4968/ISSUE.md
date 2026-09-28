# #4968: REN-D8-2026-09-27-02: The nuclear surface-light dimmer scans the unfiltered scene volume list, while the grid now culls distant sources

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4968
- **Labels**: low,renderer,shaders,bug

_Filed from `docs/audits/AUDIT_RENDERER_2026-09-27.md` (full renderer audit, all 12 dimensions, audited `main` @ `7e9da5dcc`; code at the finding's location unchanged at `fe80f4d76`). Finding ID: **REN-D8-2026-09-27-02**._

- **Severity**: LOW. A narrow trigger: a live nuclear source farther than `grid_far + radius` from the camera while another combustion source burns near the camera. Visual only.
- **Dimension**: Volumetrics
- **Location**: `crates/renderer/src/vulkan/volumetrics.rs` `append_combustion_surface_lights` (`let nuclear_source_active = source_volumes.iter().any(…FOG_VOLUME_PROFILE_EXPLOSION_NUCLEAR…)`, ~l.1935). Its caller `assemble_camera_and_lights.rs` passes the raw `fog_volumes`, and `post_passes.rs` `record_volumetrics_pass` filters through `filter_fog_volumes_for_grid` (`88c23887b`).
- **Status**: NEW. The scan itself predates the window. `88c23887b` established the contract it now contradicts ("A distant source must not arm the grid-wide simulation"), and applies that contract only to the dispatch and transport gate.
- **Description**:
  - Every combustion light decoded from this slot's moments is scaled by `NUCLEAR_COMBUSTION_SURFACE_LIGHT_SCALE` (0.22), and its cull radius by √0.22, whenever *any* nuclear volume exists anywhere in the submitted list.
  - A nuclear source that `filter_fog_volumes_for_grid` culls contributes no moments, because the field is camera-centred.
  - It still dims every nearby fire, explosion or smoke-emission light to 22 % with 47 % reach.
- **Impact**: A campfire or burning wreck near the player loses most of its field-derived surface light while a distant Fat Man cloud is alive (FO3/FNV/FO4 exteriors).
- **Suggested Fix**: Evaluate `nuclear_source_active` against the grid-filtered set, which is already in `volumetric_fog_scratch` from the previous use of the slot. Better, attribute the scale per moment bin by distance to a nuclear source.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders, other producers/consumers, other games)
- [ ] **TESTS**: A regression test pins this specific fix (and fails when the guarded code is deleted — mutation-check it)
