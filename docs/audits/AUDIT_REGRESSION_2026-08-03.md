# Regression Verification Audit — 2026-08-03

**Scope**: Verify that previously-closed bug fixes have not regressed. Discovery
window: `gh issue list --state closed --label bug --limit 150` (covers #2043–#2260,
the repo's most recent closed-bug window; older history sampled via the skill's
explicit "fresh verification candidates" list), plus the unconditional Step 4
fragile-area checks (NIFAL boundary, Disney BSDF/GPU-struct contracts) that are
never surfaced by GitHub-issue discovery because they guard refactors, not filed
bugs.

**Method**: five parallel verification passes (NIFAL wave, renderer/shader wave,
pex/decompiler-safety wave, ECS+concurrency+save wave, per-game/asset-pipeline
wave), each walking the fix → guard-test chain (`gh issue view` → `git log --grep`
→ read live code → run guard test) per `.claude/commands/audit-regression.md`.
One FAIL surfaced by the ECS/save-wave pass was independently re-verified by
running the exact clippy commands myself before inclusion below (see #1).

**Totals**: 41 items checked (40 individually-numbered issues + the unconditional
Step 4 checklist). **39 PASS, 0 PARTIAL, 2 issues surfaced as findings** (1
confirmed regression, 1 new sibling break of the same CI invariant).

---

## Findings

### REG-2026-08-03-01: `post_passes.rs` split (#2258) reintroduces undocumented unsafe blocks — regression of #2131 / #1904
- **Severity**: MEDIUM
- **Dimension**: Regression / Unsafe-Block Discipline
- **Location**: `crates/renderer/src/vulkan/context/post_passes.rs:254,311,417,620,663,730,768,796,864`
- **Status**: Regression of #2131 (itself a regression of #1904)
- **Description**: #1904 flagged ~134 renderer `unsafe {}` blocks with no `SAFETY:` comment and recommended `#![deny(clippy::undocumented_unsafe_blocks)]` (now live at `crates/renderer/src/lib.rs:21`). #2131 was a first regression of that fix (30 blocks went undocumented again) and was closed by commit `b0d331af`, which added per-block `// SAFETY:` comments. Today's commit `7bb517b2` ("Fix #2258: split record_post_passes into one helper per GPU pass") split the 556-LOC `record_post_passes` into nine `record_<pass>_pass` helpers, moving the unsafe FFI calls verbatim into each helper — but instead of preceding each `unsafe {` with its own `// SAFETY:` line, the commit put a `# Safety` **doc comment on the enclosing safe function**. `clippy::undocumented_unsafe_blocks` does not accept a function-level doc comment as satisfying the lint for a block inside a *safe* fn (that convention only applies to `unsafe fn`); it requires a comment on the line(s) immediately preceding the `unsafe {` itself. Only one of the ten new/moved blocks (`post_passes.rs:119`, the depth-history barrier helper) got an inline `// SAFETY:` comment — the other nine (in `record_svgf_pass`, `record_caustic_splat_pass`, the volumetrics helper, `record_taa_pass`, `record_ssao_pass`, `record_bloom_pass`, `record_composite_pass`, `record_upscale_pass`, `record_presentation_pass`) did not.
- **Evidence**:
  ```
  $ cargo clippy -p byroredux-renderer --lib -- -D warnings 2>&1 | grep -c "unsafe block missing a safety comment"
  9
  ```
  Representative site (`record_svgf_pass`):
  ```rust
  /// until a real lost-device repro. See #479.
  ///
  /// # Safety
  /// `cmd` is in the recording state — opened by `begin_command_buffer`
  /// in `draw_frame` and not yet closed — and this runs once per frame
  /// between the main render pass end and `end_command_buffer`, at the
  /// fixed position `record_post_passes` calls it from.
  fn record_svgf_pass(&mut self, cmd: vk::CommandBuffer, frame: usize) {
      unsafe {
          if let Some(ref wca) = self.water_caustic_accum {
  ```
  This is the exact pattern #2131's own writeup called out as insufficient — a doc comment on the outer function does not satisfy the lint the crate denies.
- **Impact**: `cargo clippy -p byroredux-renderer --lib -- -D warnings` currently fails with 9 `undocumented_unsafe_blocks` errors (plus 5 unrelated pre-existing `doc_lazy_continuation` errors in the same file and in `water.rs`, not part of this regression). Any CI job gating merges on `cargo clippy --workspace -- -D warnings` is currently red because of this file. Not a runtime-safety issue — all nine blocks are ordinary ash FFI calls in a well-understood recording context — but it is the third time this same discipline has regressed in three sibling refactors (#1904 → #2131 → this), and each regression makes the *next* file-split more likely to drop the convention too, since the doc-comment shortcut now has two precedents in the tree.
- **Related**: #2131, #1904. Sibling issue: REG-2026-08-03-02 below (different lint, same "clippy --workspace -D warnings must stay green" contract, broken by a different same-day commit).
- **Suggested Fix**: Add a one-line `// SAFETY:` comment immediately before each of the nine `unsafe {` blocks in `post_passes.rs`, restating (not just cross-referencing) the precondition already captured in the function's `# Safety` doc comment — the two aren't redundant, since only the inline one satisfies the lint. Given this is now a 3rd occurrence, consider a repo-local pre-commit or CI check that runs `cargo clippy -p byroredux-renderer -- -D clippy::undocumented_unsafe_blocks` specifically (not just as part of a broader `-D warnings` sweep that's easy to skip locally) so a future split can't land without it.

---

### REG-2026-08-03-02: `cinematic.rs` keyed-lerp refactor (#2260) introduces a `redundant_closure` clippy error
- **Severity**: LOW
- **Dimension**: Regression / CI Gate Hygiene
- **Location**: `crates/scripting/src/cinematic.rs:456`
- **Status**: NEW (same-day break of the `cargo clippy --workspace -- -D warnings` gate that #2131 already exists to protect, via a different lint)
- **Description**: Commit `bb98428a` ("Fix #2260: extract shared keyed-lerp helper for sample_scalar/sample_color") introduced a shared `sample_keyed<T, V>` generic helper taking `time_of`/`value_of` field-accessor closures. The terminal "last key" fallback line passes a closure that just forwards to `value_of`:
  ```rust
  keys.last().map_or(default, |key| value_of(key))
  ```
  Clippy's `redundant_closure` lint (part of the standard `-D warnings` set, not a NIFAL/renderer-specific lint) flags this — the closure can be replaced with `value_of` itself (`keys.last().map_or(default, value_of)`).
- **Evidence**:
  ```
  $ cargo clippy -p byroredux-scripting --lib -- -D warnings 2>&1 | tail -8
  error: redundant closure
     --> crates/scripting/src/cinematic.rs:456:33
      |
  456 |     keys.last().map_or(default, |key| value_of(key))
      |                                 ^^^^^^^^^^^^^^^^^^^ help: replace the closure with the function itself: `value_of`
  error: could not compile `byroredux-scripting` (lib) due to 1 previous error
  ```
- **Impact**: Cosmetic — no behavior change, but it's a second, independent way `cargo clippy --workspace -- -D warnings` is red right now (alongside REG-2026-08-03-01), landing in the same commit-of-the-day batch (#2258/#2259/#2260/#2261 were all closed together per the session's recent-commits list). Anyone running the full workspace clippy gate today gets two unrelated failures and may misdiagnose them as one issue.
- **Related**: REG-2026-08-03-01 (same broken invariant, different site/lint, same day).
- **Suggested Fix**: `keys.last().map_or(default, value_of)` — one-line fix, `value_of: impl Fn(&T) -> V` is already the right shape to pass directly.

---

## Verified — no regression (39 items)

### NIFAL canonical-translation wave (7 issues + 3 unconditional checks) — all PASS

| Issue | Fix commit | Fix site | Guard test | Result |
|---|---|---|---|---|
| #2203 NIFAL-D6-01 (`resolve_compressed_mesh` /3 divisor) | `3b922734` | `crates/nif/src/import/collision/shape.rs:530-540` | `compressed_mesh_indices_are_vertex_indices` | PASS |
| #2204 NIFAL-D6-02 (bhkBoxShape sign) | `3b922734` | `shape.rs:139-150` | `box_extents_permute_axes_without_position_sign` | PASS |
| #2205 NIFAL-D3-01 (LightKind/direction/cone discarded) | `1a6296e2` | `crates/core/src/ecs/components/light.rs` + `byroredux/src/render/lights.rs` | `directional_light_source_uploads_as_directional_gpu_light`, `spot_light_source_uploads_as_spot_gpu_light_with_cosine_angle` | PASS |
| #2206 NIFAL-D4-02 (billboard_mode dropped) | `4fd214aa` | `ImportedMesh::billboard_mode` (`crates/nif/src/import/types.rs`), consumed `byroredux/src/cell_loader/spawn.rs:1270` | `import_propagates_billboard_mode_to_descendant_meshes` +2 | PASS |
| #2207 NIFAL-D6-03 (void TriMesh returns `Some`) | `3b922734` | `shape.rs` `finish_trimesh()` | `collision_trimesh_resolvers_drop_geometry_without_triangles` | PASS |
| #2208 NIFAL-D6-04 (strip-tail indices dropped) | `3b922734` | `shape.rs:567-577` | `compressed_mesh_preserves_strip_tail_triangle_list` | PASS |
| #2209 NIFAL-D2-02 (chunk-strip panic) | `3b922734` | `shape.rs:545-552` | `compressed_mesh_overlong_strip_table_does_not_panic` | PASS |

Unconditional checks: single material boundary (`translate_material` in `byroredux/src/material_translate.rs:84` remains the only site; `Material::metalness`/`roughness` still plain `f32`) — PASS. Typed particle emitters (`NiPSysEmitter*`/`NiPSysGrowFadeModifier` still typed-dispatched, feeding `extract_emitter_params`/`extract_emitter_rate` → `apply_emitter_params`) — PASS. Collision shape coverage (`BhkMultiSphereShape`, `BhkConvexListShape` still resolve to `CollisionShape`, not `None`) — PASS.

One nuance checked and found benign: `byroredux/src/cell_loader/references/import.rs:196-200` still hardcodes `placement_root_billboard: None`. This looked at first glance like #2206 reappearing, but the live fix took a different (better) path — per-mesh `ImportedMesh::billboard_mode` propagated through `walk_node_flat` — so the whole-scene-graph field staying `None` is expected and doesn't affect the 3 passing #2206 guard tests or the live consumer at `spawn.rs:1270`.

### Renderer/shader wave (7 issues + unconditional checks) — all PASS

| Issue | Fix commit | Fix site | Guard | Result |
|---|---|---|---|---|
| #2116 REN-D14-01 (caustic SSBO mis-index) | `2cd44502` | `caustic_splat.comp:188` | none (shader-only) | PASS |
| #2217 REN-2026-07-28-01 (causticLum source/SPIR-V drift) | `4d7abd28` | `composite.frag:453` | `caustic_luminance_combines_both_accumulators_in_float_fixed_point`; fresh `glslangValidator` recompile byte-diffed identical to committed `.spv` | PASS |
| #2224 REN-D2-01 (fire-refraction shadow occlusion) | `291c78b0` | `shadow_transport.glsl:36-37` (`MATERIAL_KIND_FIRE_REFRACTION` skip) | none (shader-only) | PASS |
| #2227 REN-D1-01 (SHADOW_MASK_OPAQUE doc) | `c55fb12c` | `predicates.rs:594-598` (docs-only per issue scope) | `shadow_mask_bucket_selection_is_pinned` | PASS |
| #2245 REN-D19-01 (perturbNormal handedness double-flip) | `b789ef1d` | `material_sampling.glsl:187-193` | none (shader-only) | PASS |
| #2246 REN-D19-02 (Starfield bitangent sign) | `d14557be` | `bs_geometry.rs:~178` | `tangent_extraction_normalizes_off_nominal_sign_to_exact_plus_or_minus_one` | PASS |
| #2165 D2-01 (two-sided blend split, regression of #1804) | `8e55a714` | `draw.rs:1063-1065` — gate now `DrawBatch::order_dependent_glass`, not the `z_write` proxy | 7 tests incl. the previously-inverted `does_not_split_two_sided_blended_particles` | PASS, no re-regression |

Unconditional checks: Disney/Burley split still isolated in `crates/renderer/shaders/include/pbr.glsl` with GLSL-PathTracer MIT attribution intact both there and atop `triangle.frag` — PASS. `resRadiance[]` retirement (#1369) holds — no live per-thread reservoir array anywhere in `crates/renderer/shaders/`, `shadowableLightRadiance` live in `lighting.glsl:71` — PASS. `cargo test -p byroredux-renderer gpu_`: 39 passed including `gpu_instance_is_128_bytes_std430_compatible` and `gpu_camera_is_336_bytes` — PASS.

### Pex/decompiler-safety + misc wave (7 items) — all PASS

| Item | Fix commit | Fix site | Guard |
|---|---|---|---|
| #1815 (recursion-depth cap) | `7fdb694b` | `crates/pex/src/decompile/boolean.rs` `MAX_REBUILD_DEPTH=1024` | `rebuild_rejects_excessive_recursion_depth` — PASS |
| #1816 (`translate_pex` catch_unwind) | `8b04c492` | `crates/scripting/src/translate/mod.rs:111` | 3 `translate_pex_on_*` tests — PASS |
| #1728 (Skyrim-BE/Starfield round-trip) | `ae219630` | `crates/pex/src/lib.rs` `PexWriter` big_endian + guard fixtures | both round-trip tests — PASS |
| #1740 (DA10 byte-equality parity) | `2f0b99fa` | `crates/scripting/tests/pex_recognize_e2e.rs` | `da10_pex_reproduces_hand_builder_byte_for_byte` (run with `--ignored` against real Skyrim SE data) — PASS |
| #1731 (VWD flag parse+expose) | `175ebf2c` | `crates/plugin/src/esm/reader.rs:27` `FLAG_VISIBLE_WHEN_DISTANT` | 2 `parse_stat_*_vwd_flag` tests — PASS |
| #1718 (ragdoll drop telemetry) | `ffe9a816` | `byroredux/src/ragdoll.rs:81` `template_from_imported` | `dropped_bone_excludes_body_and_dependent_constraint_but_keeps_the_rest` — PASS |
| #1651 (WRONG fix) / #1823 (revert) | `27334481` | `byroredux/src/asset_provider/material.rs:620` `bgsm_blend_to_gamebryo` — plain narrowing cast, no GL-enum swap | `bgsm_merge_forwards_alpha_blend_mode` — PASS; confirmed `gl_to_gamebryo_blend` no longer exists anywhere in the tree |

### ECS + concurrency + save wave (4 of 5 PASS; #2131 reported above)

| Issue | Fix commit | Fix site | Guard |
|---|---|---|---|
| #2147 (SeatReservations wholesale clear) | `0dcb71b7` | `byroredux/src/cell_loader/references/mod.rs:308,1443` `prune_seat_reservations()` | 4 `seat_reservation_prune_tests` — PASS |
| #2148 (SparseSetStorage never shrinks) | `0dcb71b7` | `crates/core/src/ecs/sparse_set.rs` (`EMPTY` sentinel, `shrink_sparse_tail`), `world.rs:256` | 6 `sparse_footprint_tests` — PASS |
| #2149 (tracker defuse ordering) | `0dcb71b7` | `crates/core/src/ecs/world.rs` — construct `QueryRead`/`QueryWrite` before `scope.defuse()` | 2 `tracker_defuse_ordering_tests` — PASS |
| #2181 (serde(default) line-prefix guard) | `8709e12d` | `byroredux/src/save_io.rs:1748` `serde_attr_declares_default()` | 6 tests incl. `serde_default_on_saved_struct_requires_format_major_bump` — PASS |

### Per-game / asset-pipeline / perf wave (14 issues) — all PASS

| Issue | Fix commit | Fix site | Guard |
|---|---|---|---|
| #2078 (NifImportRegistry cache key) | `eda7ee39` | `byroredux/src/debug_load.rs:211-253` | `archive_set_change_clears_nif_registry` — PASS |
| #2083 (ragdoll re-activation leak) | `d60a62ee` | `byroredux/src/ragdoll.rs:282-296` | `reactivating_ragdoll_does_not_leak_previous_bodies` — PASS |
| #2091 (FO4 alpha-test inert) | `90d1e76a` | `crates/nif/src/import/material/dedicated_shader.rs:~278` | `fo4_alpha_test_flag_seeds_threshold_over_blend_only_alpha_property` — PASS |
| #2092 (FO4 Skin Tint alpha discarded) | closed as already-fixed | `shader.rs:604/1429` → `material/mod.rs:656/1175` | code-confirmed by direct read |
| #2105 (BSWeakReferenceNode gap) | `b7e0318f`→`e3b9b115` | `crates/nif/src/blocks/node.rs:936` `SF_WEAK_REF_GAP` | `bs_weak_reference_node_parses_populated_lists_with_undocumented_gap` — PASS |
| #2106 (two-digit mesh series) | `b7e0318f` | `byroredux/src/asset_provider/archive.rs:343,353-362` | 2 sibling-series tests — PASS |
| #2201 (regression of #2105) | `e3b9b115` | `node.rs:894,936` split `SF_FORM_ID`(173)/`SF_WEAK_REF_GAP`(175) gates | 2 gate-separation tests — PASS, correctly re-fixed |
| #2168 (NiSkinData version gates) | `22798ecc` | `crates/nif/src/blocks/skin.rs:102-117`, `version.rs:228/243` | `skin_data_layout_gates_carry_both_nif_xml_bounds` — PASS |
| #2160 (particle/entity motion-history collision) | `11ae4a35` | `crates/renderer/src/vulkan/context/draw.rs:144` `uses_rigid_motion_history` | 3 `rigid_motion_history_tests` — PASS |
| #2141 (SSAO resize failure binding) | `8e0e2cf9` | `resize.rs:449-476` | `ssao_recreate_failure_rebinds_binding_7_to_the_placeholder` — PASS |
| #2142 (water-caustic resize failure binding) | `8e0e2cf9` | `resize.rs:688-713` | `water_caustic_rebind_is_not_gated_on_accumulator_presence` — PASS |
| #2156 (upscaler-mode-switch soft-lock) | `c881b4c8` | `resize.rs:1143-1172`, `byroredux/src/app_step.rs` | 2 tests — PASS |
| #2157 (one-time cmd buffer leak) | `c881b4c8` | `crates/renderer/src/vulkan/texture.rs:705-742` | `one_time_commands_free_cmd_buffer_on_every_error_path` — PASS |
| #2158 (FrameUpscaler teardown ordering) | `c881b4c8` | `frame_upscaler.rs` split teardown, `context/mod.rs:3432,3541` | `fsr_context_teardown_sits_outside_the_allocator_guard` — PASS |

---

## Summary Table

| Issue/Item | Title | Status | Fix Present | Guard |
|---|---|---|---|---|
| #2203–#2209 | NIFAL collision-translation wave (7) | PASS | Yes | 7 tests, all pass |
| NIFAL unconditional (3) | material boundary, particle emitters, collision coverage | PASS | Yes | code-confirmed |
| #2116,#2217,#2224,#2227,#2245,#2246 | Renderer/shader wave (6) | PASS | Yes | tests where automatable |
| #2165 | Two-sided blend split (regression of #1804) | PASS | Yes | 7 tests |
| Renderer unconditional (2) + gpu_ sweep | Disney BSDF split, resRadiance retirement, GPU struct pins | PASS | Yes | 39 `gpu_*` tests pass |
| #1815,#1816,#1728,#1740,#1731,#1718 | pex/decompiler-safety wave (6) | PASS | Yes | tests, incl. 1 real-data `--ignored` |
| #1651/#1823 | BGSM blend-factor revert | PASS | Yes (revert confirmed live) | 1 test |
| #2147,#2148,#2149 | ECS storage/query wave (3) | PASS | Yes | 12 tests |
| #2181 | save serde(default) guard | PASS | Yes | 6 tests |
| **#2131** | **Undocumented unsafe blocks (regression of #1904)** | **FAIL** | **No — re-regressed by `7bb517b2` today** | clippy `-D warnings`: 9 new errors |
| #2078,#2083,#2091,#2092,#2105,#2106,#2201,#2168,#2160,#2141,#2142,#2156,#2157,#2158 | Per-game/asset-pipeline/perf wave (14) | PASS | Yes | tests, all pass |
| n/a | `cinematic.rs` redundant_closure (new, sibling break of same CI gate) | NEW | n/a | clippy `-D warnings`: 1 new error |

**Totals**: 40 numbered issues + 5 unconditional-check groups verified. **39 PASS. 1 confirmed regression (#2131). 1 new sibling finding (same CI invariant, different lint/site).**

---

## Next Steps

```
/audit-publish docs/audits/AUDIT_REGRESSION_2026-08-03.md
```
