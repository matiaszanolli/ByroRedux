# #5507: REN-D2-2026-10-09-01: #5369's intensity-blind rig key is folded in array order, but the light array is sorted by an intensity-derived score — flicker among two or more lights re-keys the rig, so mode 3 rarely holds in the multi-light scenes it…

**Labels**: bug, medium, renderer, shaders

**Source**: `docs/audits/AUDIT_RENDERER_2026-10-09.md` — finding `REN-D2-2026-10-09-01` · suite `/audit-suite --preset streaming-deep` (2026-10-09) · HEAD `3bcf6c8e8`

- **Severity**: MEDIUM
- **Dimension**: Soft Shadows / Light Animation (ReSTIR direct EMA)
- **Location**:
  - `crates/renderer/src/vulkan/caustic.rs`, `fold_light_rig_geometry_key_for`, which uses the order-dependent FNV-1a step `fold_caustic_key_f32`.
  - `crates/renderer/src/vulkan/context/build_and_upload_instances.rs`: the `restir_rig_key` loop over `lights` and the `restir_rig_static` gate.
  - The sort sites: `sort_lights_by_priority_with_ids` (`scene_buffer/light_history.rs`), called from `assemble_camera_and_lights.rs` and from `collect_lights` (`byroredux/src/render/lights.rs`).
  - The score `GpuLight::gi_priority_score` (`scene_buffer/gpu_types.rs`).
- **Status**: NEW. It is the residual of #5369 (CLOSED by "Fix #5369 (partial)"). Related: **#5370** (OPEN, HIGH — FO4 Institute speckle).
- **Description**:
  - The rig key deliberately excludes colour, intensity and cull radius, so that flicker keeps the deep parked EMA (history mode 3).
  - It folds the lights in the order of the uploaded array, and that array is sorted by `gi_priority_score = (r + g + b) × position_radius.w`. That score is exactly the intensity family the key excludes.
  - `flicker_intensity` (`byroredux/src/systems/light_anim.rs`) is per-entity seeded noise at about 12 Hz. Two flickering point lights of similar score therefore cross in sort order several times a second.
  - Each crossing changes the key, so that frame sets `restir_rig_static = false` and the mode falls to 1.
  - A mode-1 frame stores `histLen = min(histPrev + 1, 16)` (`triangle.frag`, the `historyCap` block). The deep tail then restarts from 16 after every swap and needs about 240 swap-free frames to reach the 0.008 floor again.
- **Evidence**:
  - `light_rig_geometry_key_ignores_intensity_but_not_geometry` covers a single light only, so a permutation is invisible to it.
  - The key loop runs after the post-combustion re-sort (`assemble_camera_and_lights.rs`, `scene_buffer::sort_lights_by_priority_with_ids(&mut frame_lights, …)`), whose comparator is `b.0.total_cmp(&a.0)` on the decorated score.
- **Impact**:
  - Rooms with several identical flickering fixtures keep their effective EMA well above the deep floor: candles and sconces from one LIGH base, and FO4 fluorescent banks.
  - That leaves the standing penumbra speckle #5369 set out to remove, and is plausibly part of why #5370 persists.
  - The single-light Cornell fire-lab A/B the commit measured cannot show it.
  - Visual only.
- **Related**: #5369, #5370, #4942, #5020.
- **Suggested Fix**:
  - Make the key order-independent: hash each light's geometry separately, then combine the per-light hashes commutatively (sum, or sort then fold).
  - Alternatively key by the CPU-side light identity (`light_ids`) rather than array position.
  - Add a two-light permutation case to the test.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other walkers / record decoders / games / call sites)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix
