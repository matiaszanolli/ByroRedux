# Regression Verification Audit — 2026-07-16

## Scope

- **Step 1 discovery**: `gh issue list --repo matiaszanolli/ByroRedux --state closed --label bug --limit 50` (37 unique titles after de-duplicating accidental script-rerun duplicates — see Dedup Note below), plus an explicit `--issues` pass over the skill's named "fresh verification candidates" (decompiler-safety + LC wave: #1815, #1816, #1728, #1740, #1731, #1718) and the #1823/#1651 special case.
- **Step 4 unconditional fragile-area checks**: NIFAL single material boundary, typed particle emitters, collision shape coverage, Disney BSDF/ReSTIR retirement, and GPU struct size-pin tests — run regardless of Step 1's discovery window, per the skill.
- **Excluded**: #1947 (REN-D2-01) was closed `NOT_PLANNED` (the finding's premise was disproven, not fixed) — nothing to regress, so it is not included below.
- Work was split across 6 parallel verification passes (labeled A–F below) covering 43 distinct issues plus the 5 Step 4 checks.

**Dedup note**: 13 of the 50 raw closed-bug results were exact-title duplicates closed `NOT_PLANNED` with a comment attributing them to "accidental script re-run during audit publishing" (e.g. #1972 is a duplicate of #1939). These were collapsed to their `COMPLETED` sibling before verification; only the real, fixed issue is listed below.

## Per-Issue Findings

### Batch A — NIF/parser (10 issues)

## #1882: NIF-D1-002: BSWeakReferenceNode under-consumes 2 bytes on every Starfield node
- **Status**: PASS
- **Closed**: 2026-07-05
- **Fix commit**: `550ff215`
- **Fix site**: `crates/nif/src/blocks/node.rs` (`BsWeakReferenceNode::parse_with_size`, captures `starfield_tail: Vec<u8>` up to `block_size`)
- **Fix present**: Yes
- **Guard test**: `bs_weak_reference_node_captures_starfield_trailing_tail` in `crates/nif/src/blocks/dispatch_tests/nodes.rs` — passes
- **Notes**: None.

## #1883: NIF-D3-001/002/003: per-block coverage gate false-green blind spot
- **Status**: PARTIAL
- **Closed**: 2026-07-05
- **Fix commit**: `82921415`
- **Fix site**: `crates/nif/tests/common/mod.rs` (`compare_histograms` — now iterates the union of baseline+current keys)
- **Fix present**: Yes (NIF-D3-002 only)
- **Guard test**: `new_type_landing_in_unknown_is_a_regression` in `crates/nif/tests/common/mod.rs` — passes
- **Notes**: Only NIF-D3-002 (union-key iteration) was actually fixed. The commit message explicitly defers NIF-D3-001 (alias-collapse of ~28 `NiPSys*Modifier` types into one `NiPSysBlock` bucket — confirmed still present in `particle.rs`) and does not address NIF-D3-003 (single-archive coverage scope) at all. **The issue was closed as if all three sub-findings were resolved; two of three remain open in the live tree.** Not a regression (nothing that was fixed broke), but the closure overstates the fix — flagged as a process gap, see Process Notes below.

## #1885: NIF-D6-001: interpolator.rs::parse_legacy uses raw Vec::with_capacity
- **Status**: PASS
- **Closed**: 2026-07-06
- **Fix commit**: `155852e3`
- **Fix site**: `crates/nif/src/blocks/interpolator.rs` (`parse_legacy`, now `stream.allocate_vec::<InterpBlendItem>(array_size as u32)?`)
- **Fix present**: Yes
- **Guard test**: `parse_legacy_blend_interpolator_rejects_oversized_array_size` in `crates/nif/src/blocks/interpolator_tests.rs` — passes
- **Notes**: None.

## #1887: FO3-D3-001: XATO REFR arm is FONV-only — benign FNV misparse + unsourced comment
- **Status**: PARTIAL
- **Closed**: 2026-07-05
- **Fix commit**: `7e6122c4`
- **Fix site**: `crates/plugin/src/esm/cell/walkers.rs` (`XATO` arm comment, ~line 938)
- **Fix present**: Yes (comment/provenance-correction only)
- **Guard test**: none — fix is comment-only, no behavioral change to pin. Existing #584 guard `parse_refr_extracts_fo4_texture_override_subrecords` in `crates/plugin/src/esm/cell/tests/refr.rs` still passes but predates and is unrelated to this issue.
- **Notes**: The commit explicitly defers the behavioral fix (game-gating XATO/XTNM/XTXR to FO4+) pending `GameKind` threading through `parse_refr_group` — documented future work, not a regression.

## #1896: NIF-D1-01: Three FO76 particle blocks silently drop their #BS_F76# fields
- **Status**: PASS
- **Closed**: 2026-07-06
- **Fix commit**: `b20e4863`
- **Fix site**: `crates/nif/src/blocks/particle.rs` (`parse_particles_data` +12B Vector3 gate ~line 1279; `parse_rotation_modifier` +17B gate ~line 409; `BSPSysSimpleColorModifier::parse` +52B gate ~line 591), all gated on `stream.bsver() == crate::version::bsver::FO76`
- **Fix present**: Yes, all 3 sites
- **Guard test**: `parse_rotation_modifier_reads_fo76_interleaved_fields` and `bs_simple_color_modifier_consumes_fo76_trailer` in `crates/nif/src/blocks/particle.rs` — both pass. Live-corpus `parse_rate_fallout_76` (`#[ignore]`, run explicitly) also passes at 100% clean.
- **Notes**: `NiPSysData`'s +12B field has no dedicated byte-exact unit test (only the other two blocks do); only indirectly covered by the corpus-level clean-rate test.

## #1898: NIF-D2-06: Max Filepath BSStreamHeader gate uses raw magic literal 103
- **Status**: PARTIAL
- **Closed**: 2026-07-11
- **Fix commit**: `7d00348b`
- **Fix site**: `crates/nif/src/header.rs:158` (`if user_version_2 >= bsver::MAX_FILEPATH`), constant defined in `crates/nif/src/version.rs:348`
- **Fix present**: Yes
- **Guard test**: none dedicated — commit message notes "pure rename, no behavior change." No test can distinguish the named constant from a reintroduced bare literal since behavior is identical either way.
- **Notes**: Cosmetic-only fix; inherently unguardable by a behavioral test.

## #1899: NIF-D3-01: Oblivion per-block TSV baseline stale-high on NiUnknown
- **Status**: PASS
- **Closed**: 2026-07-11
- **Fix commit**: `7d00348b`
- **Fix site**: `crates/nif/tests/data/per_block_baselines/oblivion.tsv` (`NiMaterialProperty` and `NiTexturingProperty` rows now show `0` unknown, down from `1`)
- **Fix present**: Yes
- **Guard test**: `per_block_baseline_oblivion` (`#[ignore]`, requires game data) in `crates/nif/tests/per_block_baselines.rs` — ran with `--ignored`, passes (0 unknown blocks across 81 types)
- **Notes**: None.

## #1900: NIF-D3-02: Per-game clean-rate matrix stale-low; Starfield floors erode with it
- **Status**: PASS
- **Closed**: 2026-07-12
- **Fix commit**: `208961c6`
- **Fix site**: `crates/nif/tests/parse_real_nifs.rs` (Starfield `min_clean` floors tightened: Meshes01 0.970→0.995, Meshes02 0.990→0.995, MeshesPatch 0.970→0.980)
- **Fix present**: Yes
- **Guard test**: `parse_rate_starfield_all_meshes` (`#[ignore]`, requires game data) in `crates/nif/tests/parse_real_nifs.rs` — ran with `--ignored`, passes
- **Notes**: None.

## #1901: NIF-D5-01: parse_fo4 backlight-power presence threshold is 3.0e38, not FLT_MAX
- **Status**: PARTIAL
- **Closed**: 2026-07-06
- **Fix commit**: `b20e4863`
- **Fix site**: `crates/nif/src/blocks/shader.rs:1029` (`BSLightingShaderProperty::parse_fo4`, now `rim >= f32::MAX && rim.is_finite()`)
- **Fix present**: Yes
- **Guard test**: none for the specific fix. `parse_bs_lighting_fo4_finite_rimlight_skips_backlight` in `crates/nif/src/blocks/shader_tests.rs` exists but tests `rim=2.5` (clearly finite, outside the `[3.0e38, f32::MAX)` gap the issue targeted) — a pre-existing #1175 test, not a new one for #1901.
- **Notes**: Guard gap — a regression reverting the threshold back to `3.0e38` would not be caught by any existing test.

## #1902: NIF-D6-01: BhkMultiSphereShape::parse fills Vec<[f32;4]> with a per-element push loop
- **Status**: PASS
- **Closed**: 2026-07-12
- **Fix commit**: `208961c6`
- **Fix site**: `crates/nif/src/blocks/collision/shape_primitive.rs` (`BhkMultiSphereShape::parse`, now `stream.read_ni_color4_array(num_spheres as usize)?`)
- **Fix present**: Yes
- **Guard test**: `bhk_multi_sphere_shape_consumes_full_52_bytes_for_2_spheres` in `crates/nif/src/blocks/dispatch_tests/havok.rs` (pre-existing #394 test, reused) — passes
- **Notes**: None.

**Batch A summary: 5 PASS, 5 PARTIAL, 0 FAIL, 0 UNVERIFIABLE.**

### Batch B — Safety/scripting (7 issues)

## #1903: SAFE-D2-01: Header-parser read_sized_string allocates unbounded (residual #388 gap)
- **Status**: PASS
- **Closed**: 2026-07-06
- **Fix commit**: `b20e4863`
- **Fix site**: `crates/nif/src/header.rs` (`check_header_alloc`, gating `read_sized_string`)
- **Fix present**: Yes — `check_header_alloc(len, cursor)?` called at `header.rs:422` before the `vec![0u8; len]` allocation
- **Guard test**: `header::tests::check_header_alloc_rejects_oversized_len`, `header::tests::read_sized_string_rejects_corrupt_length` in `crates/nif/src/header.rs` — both pass
- **Notes**: None.

## #1904: SAFE-D4-01: ~134 renderer FFI unsafe {} blocks carry no SAFETY comment (batched)
- **Status**: PASS
- **Closed**: 2026-07-14
- **Fix commit**: `332c0230`
- **Fix site**: `crates/renderer/src/vulkan/*` (all `unsafe {}` blocks), lint gate at `crates/renderer/src/lib.rs:21` (`#![deny(clippy::undocumented_unsafe_blocks)]`)
- **Fix present**: Yes — spot-checked `water.rs` (11 SAFETY comments) and `buffer.rs` (25); crate-root deny lint confirmed present.
- **Guard test**: no `cargo test` unit test (documentation-only fix); guard mechanism is `cargo clippy -p byroredux-renderer -- -W clippy::undocumented_unsafe_blocks`, run and produced **zero** undocumented-unsafe-block warnings. Since the crate carries `#![deny(...)]`, any regression hard-fails `cargo clippy`/CI.
- **Notes**: Guard is a compile-time lint gate rather than a `#[test]`, by design.

## #1905: SCR-D5-NEW-03: quest_stage_gate drops per-predicate quest on multi-quest GetStageDone gate
- **Status**: PASS
- **Closed**: 2026-07-06
- **Fix commit**: `b3d63a2b`
- **Fix site**: `crates/scripting/src/translate/recognizers/quest_stage_gate.rs` (`classify_if_condition`, lines ~284-306) — `get_or_insert` replaced with compare-or-decline
- **Fix present**: Yes
- **Guard test**: `translate::recognizers::quest_stage_gate::tests::declines_mixed_quest_conjunction` in `crates/scripting/src/translate/recognizers/quest_stage_gate.rs` — passes
- **Notes**: None.

## #1906: SCR-D4-NEW-01: Papyrus int/float literal regexes swallow a leading minus
- **Status**: PASS
- **Closed**: 2026-07-06
- **Fix commit**: `b3d63a2b`
- **Fix site**: `crates/papyrus/src/token.rs` (`IntLit`/`FloatLit` regexes, leading `-?` removed)
- **Fix present**: Yes
- **Guard test**: `lexer::tests::test_lex_adjacent_subtraction_not_swallowed` and `parser::expr::tests::test_adjacent_subtraction` in `crates/papyrus/src/lexer.rs` / `crates/papyrus/src/parser/expr.rs` — both pass
- **Notes**: None.

## #1907: SCR-D5-NEW-04: lower_fragment silently drops a non-quest binding's side-effect
- **Status**: PASS
- **Closed**: 2026-07-06
- **Fix commit**: `f63a701e`
- **Fix site**: `crates/scripting/src/translate/effects.rs` (`bind_local` now returns `Option<()>`; new `is_side_effect_free` helper)
- **Fix present**: Yes
- **Guard test**: `translate::effects::tests::declines_on_side_effecting_binding` and `side_effect_free_binding_is_recorded_not_declined` in `crates/scripting/src/translate/effects.rs` — both pass
- **Notes**: None.

## #1908: SCR-D4-NEW-02: Out-of-range Papyrus integer/float literals silently become 0
- **Status**: PASS
- **Closed**: 2026-07-06
- **Fix commit**: `f63a701e`
- **Fix site**: `crates/papyrus/src/token.rs` (`parse_int`, `parse_float` now return `Result<_, ()>` instead of `unwrap_or(0)`/`unwrap_or(0.0)`)
- **Fix present**: Yes
- **Guard test**: `lexer::tests::test_out_of_range_literals_surface_diagnostic` in `crates/papyrus/src/lexer.rs` — passes
- **Notes**: None.

## #1909: SCR-D5-NEW-05: rumble recognizer coerces a non-literal property to its .psc default
- **Status**: PASS
- **Closed**: 2026-07-06
- **Fix commit**: `f63a701e`
- **Fix site**: `crates/scripting/src/translate/recognizers/rumble.rs` (`float_prop`/`bool_prop` three-case contract; `recognize` uses `?` to decline on `None`)
- **Fix present**: Yes
- **Guard test**: `translate::recognizers::rumble::tests::declines_on_non_literal_property` in `crates/scripting/src/translate/recognizers/rumble.rs` — passes
- **Notes**: None.

**Batch B summary: 7 PASS, 0 PARTIAL, 0 FAIL, 0 UNVERIFIABLE.**

### Batch C — Renderer, part 1 (6 issues)

## #1913: REN-D1-03: SHADOW_MASK_* → u8 truncation site has no 8-bit ceiling pin
- **Status**: PASS
- **Closed**: 2026-07-14
- **Fix commit**: `546e372e`
- **Fix site**: `crates/renderer/src/shader_constants_data.rs` (const-assert block, lines ~71-95); `crates/renderer/src/vulkan/acceleration/predicates.rs` (`shadow_mask_for_material`)
- **Fix present**: Yes — compile-time `const _: () = { assert!(...) }` pins nonzero, ≤0xFF, and distinctness for both constants
- **Guard test**: `shadow_mask_bucket_selection_is_pinned` in `crates/renderer/src/vulkan/acceleration/tests.rs` — passes
- **Notes**: Both a compile-time assert and a runtime test exist.

## #1916: REN-D2-04: GpuLight shader-struct-sync enumeration misses volumetrics_inject.comp
- **Status**: PASS
- **Closed**: 2026-07-11
- **Fix commit**: `9054506c`
- **Fix site**: `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs` (doc comment above `struct GpuLight`, now lists all four GLSL copies including `volumetrics_inject.comp`)
- **Fix present**: Yes
- **Guard test**: `gpu_light_glsl_copies_stay_in_lockstep` in `crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs` — passes (walks all four GLSL sources, asserts field-list identity)
- **Notes**: None.

## #1917: REN-D3-01: composite.frag.spv is stale
- **Status**: PASS
- **Closed**: 2026-07-11
- **Fix commit**: `9054506c`; superseded/refined by follow-up `e1b0294d` (#1926, dead-branch removal + pin update 16→12)
- **Fix site**: `crates/renderer/shaders/composite.frag.spv` (recompiled binary); guard in `crates/renderer/src/vulkan/reflect.rs`
- **Fix present**: Yes — current source has no `depth_params.z` gate around the volumetric blend (unconditional at line 454)
- **Guard test**: `composite_frag_spv_matches_recompiled_branch_count` in `crates/renderer/src/vulkan/reflect.rs` — passes (pinned at 12 `OpBranchConditional`)
- **Notes**: Guard is a SPIR-V branch-count pin, robust to future silent recompile drift.

## #1920: REN-D3-04: generated_header_contains_all_defines value-pins omit 10 defines
- **Status**: PASS
- **Closed**: 2026-07-11
- **Fix commit**: `a0b5539c`
- **Fix site**: `crates/renderer/src/shader_constants.rs` (expected-lines array in `generated_header_contains_all_defines`, lines 111-120)
- **Fix present**: Yes — all 10 named constants present (`CLUSTER_NEAR`, `CLUSTER_FAR_FLOOR`, `CLUSTER_FAR_FALLBACK`, `VERTEX_NORMAL_OFFSET_FLOATS`, `VERTEX_UV_OFFSET_FLOATS`, `SHADOW_MASK_OPAQUE`, `SHADOW_MASK_GLASS`, `GI_HIT_LIGHT_CAP`, `CAUSTIC_FIXED_SCALE`, `ENABLE_LEGACY_WRS`)
- **Guard test**: `generated_header_contains_all_defines` in `crates/renderer/src/shader_constants.rs` — passes
- **Notes**: None.

## #1921: REN-D5-01: Batched texture flush releases staging buffers with upload size, not allocation size
- **Status**: PARTIAL
- **Closed**: 2026-07-11
- **Fix commit**: `a0b5539c`
- **Fix site**: `crates/renderer/src/vulkan/texture.rs` (`record_dds_upload`, lines ~424-438) — `staging_capacity = staging.allocation.as_ref().map(|a| a.size()).unwrap_or(image_size)`, matches the issue's suggested fix verbatim; consumed correctly by `flush_pending_uploads` in `crates/renderer/src/texture_registry.rs`
- **Fix present**: Yes
- **Guard test**: none found — no test in `texture_registry_tests.rs`, `texture_registry_bindless_tests.rs`, or `texture.rs`'s own `#[cfg(test)]` block exercises the pooled-reuse capacity path
- **Notes**: Hardening gap remains open exactly as flagged — fix code is correct, but nothing pins it against a future regression (e.g. reverting to returning `image_size` unconditionally).

## #1925: MAT-D6-02: "scrap" classifier keyword is an unbounded substring match
- **Status**: PASS
- **Closed**: 2026-07-11
- **Fix commit**: `e1b0294d`
- **Fix site**: `crates/core/src/ecs/components/material.rs` (`classify_pbr_keyword`, scrap-cladding arm now `contains_any_ci(path, &["metalscrap"])`, line 482)
- **Fix present**: Yes
- **Guard test**: `classify_pbr_bare_scrap_reaches_metal_arm` + `classify_pbr_scrap_metal_is_not_chrome` in `crates/core/src/ecs/components/material.rs` — both pass
- **Notes**: Two tests cover both the cladding-still-matte case and the bare-scrap-reaches-metal case.

**Batch C summary: 5 PASS, 1 PARTIAL, 0 FAIL, 0 UNVERIFIABLE.**

### Batch D — Renderer, part 2 (7 issues)

## #1926: REN-D8-01: Composite fog fallback branch is dead code post-VOLUMETRIC_OUTPUT_CONSUMED flip
- **Status**: PASS
- **Closed**: 2026-07-11
- **Fix commit**: `e1b0294d`
- **Fix site**: `crates/renderer/shaders/composite.frag` (aerial-perspective fog fallback block, ~line 485-496)
- **Fix present**: Yes — branch removed; `fog_color`/`fog_params` stay in the UBO but are documented as reserved-and-unconsumed for a future REGN density-tint feature
- **Guard test**: `composite_frag_spv_matches_recompiled_branch_count` in `crates/renderer/src/vulkan/reflect.rs` — passes (pins `OpBranchConditional` count at 12, down from 16 pre-fix)
- **Notes**: Test doc comment traces the full 17→16→12 branch-count history across #1917 and #1926.

## #1927: REN-D8-02: #865 XCLL cubic-fog was never reachable for the interiors it targets
- **Status**: PARTIAL
- **Closed**: 2026-07-11
- **Fix commit**: `2ccfc04a`
- **Fix site**: `crates/renderer/shaders/composite.frag` (`fog_params` UBO comment) + `crates/renderer/src/vulkan/context/draw.rs` (`fog_clip`/`fog_power` doc comments, ~line 358-376)
- **Fix present**: Yes — the buggy exterior-gated/sky-haze-mixing branch this issue targeted was removed entirely by #1926 in the same session, so this fix is documentation-only
- **Guard test**: none found
- **Notes**: Not a regression — the commit message explicitly states no test was added because "the buggy code path no longer exists, so there's nothing left to pin." Legitimate PARTIAL, not a gap needing action.

## #1929: REN-D11-01: triangle.vert.spv compiled to SPIR-V 1.5 while every sibling shader is SPIR-V 1.0
- **Status**: PASS
- **Closed**: 2026-07-11
- **Fix commit**: `b01c2e38`
- **Fix site**: `crates/renderer/shaders/triangle.vert.spv` (+ `taa.comp.spv`, a second drifted file the sweep found)
- **Fix present**: Yes — recompiled with plain `-V`
- **Guard test**: `every_committed_spv_is_spirv_1_0` in `crates/renderer/src/vulkan/reflect.rs` — passes (pins all 20 committed `.spv` files to SPIR-V (1,0))
- **Notes**: Verified independently by recompiling `triangle.vert` locally — no drift.

## #1932: TAA-D13-01: Halton jitter gate omits the taa_failed check present on TAA's other two gates
- **Status**: PASS
- **Closed**: 2026-07-11
- **Fix commit**: `b01c2e38`
- **Fix site**: `crates/renderer/src/vulkan/context/draw.rs` (`fn taa_jitter` at line 69, gate: `if taa_present && !taa_failed`; call site at line 2563 passes `self.taa_failed`)
- **Fix present**: Yes
- **Guard test**: `taa_jitter_tests::{no_taa_present_is_unjittered, taa_failed_is_unjittered_even_with_pipeline_present, taa_present_and_not_failed_jitters_nonzero}` in `crates/renderer/src/vulkan/context/draw.rs` — all 3 pass
- **Notes**: Logic extracted into a pure, unit-testable helper.

## #1934: CAUSTIC-D14-01: #1234 named-macro fix in caustic_splat.comp has no regression-test coverage
- **Status**: PASS
- **Closed**: 2026-07-12
- **Fix commit**: `f03e5d4a`
- **Fix site**: `crates/renderer/src/shader_constants.rs` (`assert_no_bare_flags_literal` helper, line 403)
- **Fix present**: Yes — shader source (`caustic_splat.comp:200`) still uses `INSTANCE_FLAG_CAUSTIC_SOURCE`, not a bare `4u`
- **Guard test**: `caustic_splat_comp_uses_named_instance_flag_constant` in `crates/renderer/src/shader_constants.rs` — passes
- **Notes**: None.

## #1937: VOL-D16-01: Sun visibility ray cast in the wrong hemisphere
- **Status**: PARTIAL
- **Closed**: 2026-07-10
- **Fix commit**: `68d9c43b` (bundled with #1939)
- **Fix site**: `crates/renderer/shaders/volumetrics_inject.comp` (line 317-319: `ray_dir = -light_in` — cast toward the sun; `light_in` unchanged so the HG phase cosine stays correct)
- **Fix present**: Yes — verified against the "sun_direction points TOWARD the sun" convention
- **Guard test**: none (commit message: "no cargo-testable regression surface"); only a doc comment on `VolumetricsParams::sun_dir` in `crates/renderer/src/vulkan/volumetrics.rs:80` references #1937
- **Notes**: `volumetrics_inject.comp.spv` re-recompiled locally and diffed byte-for-byte against the committed binary — identical, no stale-spv regression. RenderDoc verification recommended by the original commit, outside cargo-test scope.

## #1939: SKY-D18-01: Effect_Lit shading path negates sun direction, inverting the hemisphere
- **Status**: PARTIAL
- **Closed**: 2026-07-10
- **Fix commit**: `68d9c43b` (same commit as #1937)
- **Fix site**: `crates/renderer/shaders/triangle.frag` (line 606-611, `MAT_FLAG_EFFECT_LIT` block, unnegated `NdotL = max(dot(N, Ldir), 0.0)` — matches the main directional path's convention)
- **Fix present**: Yes
- **Guard test**: none (same "no cargo-testable regression surface" note in the shared commit)
- **Notes**: `triangle.frag.spv` re-recompiled locally and diffed byte-identical against the committed binary — not stale. Same RenderDoc-verification caveat as #1937.

**Batch D summary: 4 PASS, 3 PARTIAL, 0 FAIL, 0 UNVERIFIABLE.**

### Batch E — Gameplay/misc + fresh decompiler-safety/LC candidates (13 issues)

## #1979: FNV-D7-01: Ragdoll non-simulated descendant bones (fingers/toes) render at the stale animated pose, detached from the crumpling body
- **Status**: PASS
- **Closed**: 2026-07-13
- **Fix commit**: `ae58a8d2` — "re-derive ragdoll non-body descendant bones from the simulated pose"
- **Fix site**: `byroredux/src/ragdoll.rs` (`ragdoll_writeback_system`, descendant BFS re-derivation block at `#1979` comment, line ~397)
- **Fix present**: Yes
- **Guard test**: `ragdoll::tests::writeback_rederives_non_body_descendant_from_simulated_parent` in `byroredux/src/ragdoll.rs` — passes
- **Notes**: None.

## #1980: ANIM-D6-01: CycleType::Reverse single-reflection ping-pong does not clamp a delta larger than 2×duration
- **Status**: PASS
- **Closed**: 2026-07-13
- **Fix commit**: `4a970d35` — "fold CycleType::Reverse ping-pong over a full period"
- **Fix site**: `crates/core/src/animation/player.rs` (`advance_time`) and `crates/core/src/animation/stack.rs` (`advance_stack`)
- **Fix present**: Yes (both sibling sites updated)
- **Guard test**: `animation::tests::advance_time_reverse_hitch_larger_than_period` in `crates/core/src/animation/player.rs` — passes
- **Notes**: The SIBLING requirement (apply fold to both `player.rs` and `stack.rs`) is satisfied structurally, though the guard test only directly exercises `advance_time`.

## #1985: FO4-D5-01: FO4 shader-flag-only alpha test is inert — threshold defaults to 0.0
- **Status**: PASS
- **Closed**: 2026-07-14
- **Fix commit**: `441186fb` — "seed FO4 shader-flag-only alpha-test threshold"
- **Fix site**: `crates/nif/src/import/material/walker.rs` (F4SF2 `ALPHA_TEST` branch, ~line 346) — seeds `info.alpha_threshold = 128.0/255.0` when no `NiAlphaProperty` consumed one
- **Fix present**: Yes
- **Guard test**: `import::material::fo4_shader_flag_tests::fo4_alpha_test_flag_sets_field` in `crates/nif/src/import/material/fo4_shader_flag_tests.rs` — passes
- **Notes**: None.

## #1986: FO4-D1-01: CSG non-final chunk length not pinned to 65536
- **Status**: PASS
- **Closed**: 2026-07-14
- **Fix commit**: `6072bb7a` — "reject short non-final CSG chunk instead of mis-addressing PSG"
- **Fix site**: `crates/bsa/src/csg.rs` (`chunk_bytes`, line ~253: exact-size assert for non-final chunks, `InvalidData` on mismatch)
- **Fix present**: Yes
- **Guard test**: `csg::tests::rejects_short_non_final_chunk` in `crates/bsa/src/csg.rs` — passes
- **Notes**: None.

## #1994: DIM2-01: Additive-blend sort key orders mesh before wireframe bit, unlike the opaque branch
- **Status**: PASS
- **Closed**: 2026-07-15
- **Fix commit**: `56019cdf` — "order additive-blend sort key by depth_state before mesh"
- **Fix site**: `byroredux/src/render/mod.rs` (`draw_sort_key`, additive-blend branch ~line 236: `pack_depth_state` now precedes `mesh_handle` when `dst_blend == GAMEBRYO_BLEND_ONE`)
- **Fix present**: Yes
- **Guard test**: `render::draw_sort_key_tests::additive_wireframe_and_fill_draws_do_not_interleave_across_meshes` in `byroredux/src/render/draw_sort_key_tests.rs` — passes
- **Notes**: Commit also folded in the #1995 sibling (stale sort-key tuple comments / magic-literal threshold cleanup) — both intact.

## #1996: DIM9-01: parse_npc never remaps embedded FormIDs (PKID/ai_packages) to global load-order space
- **Status**: PASS
- **Closed**: 2026-07-15
- **Fix commit**: `5de577b9` — "remap NPC_/CREA embedded FormIDs to global load-order space"
- **Fix site**: `crates/plugin/src/esm/records/actor.rs` (`parse_npc`, now takes `remap: &Option<FormIdRemap>` and calls `remap_fid` on every embedded FormID field); call sites `crates/plugin/src/esm/records/mod.rs` for both `NPC_` (line 488) and `CREA` (line 506)
- **Fix present**: Yes
- **Guard test**: `esm::records::actor::tests::npc_embedded_form_ids_remap_to_global_space` in `crates/plugin/src/esm/records/actor.rs` — passes
- **Notes**: Both `NPC_` and `CREA` call sites (the SIBLING requirement — CREA shares `parse_npc`) verified fixed, not just NPC_.

## #1815: SCR-D2-01: Decompiler boolean-collapse pass has no recursion-depth cap
- **Status**: PASS
- **Closed**: 2026-07-03
- **Fix commit**: `7fdb694b` — "cap recursion depth in the boolean-collapse decompiler pass"
- **Fix site**: `crates/pex/src/decompile/boolean.rs` (`BoolPass::rebuild`, `depth: usize` param + `MAX_REBUILD_DEPTH = 1024` check at line ~121, returns `DecompileError::RecursionLimit`)
- **Fix present**: Yes
- **Guard test**: `decompile::boolean::tests::rebuild_rejects_excessive_recursion_depth` in `crates/pex/src/decompile/boolean.rs` — passes; sibling `decompile::control_flow::tests::rebuild_rejects_excessive_recursion_depth` (#1729 guard) also passes
- **Notes**: None.

## #1816: SCR-D5-NEW-02: translate_pex decompiles untrusted .pex without catch_unwind
- **Status**: PARTIAL
- **Closed**: 2026-07-03
- **Fix commit**: `8b04c492` — "catch a decompiler panic in translate_pex"
- **Fix site**: `crates/scripting/src/translate/mod.rs` (`translate_pex`, line 110: `std::panic::catch_unwind(std::panic::AssertUnwindSafe(...))` wrapping `decompile_script`)
- **Fix present**: Yes
- **Guard test**: none found — `grep -rn "1816" crates/scripting/` only turns up doc comments
- **Notes**: The fix commit message states this explicitly: "no live regression test reproduces a real panic; this closes the missing safety net for future decompiler changes" — matches the corpus finding of 0/26,640 panics. Hardening gap, not a functional gap.

## #1728: SCR-D1-02: No Skyrim-BE / Starfield-guards round-trip test on an untrusted parser
- **Status**: PASS
- **Closed**: 2026-07-03
- **Fix commit**: `ae219630` — "add Skyrim-BE and Starfield-guards round-trip tests to the PEX reader"
- **Fix site**: `crates/pex/src/lib.rs` (`build_sample_skyrim_be` ~line 285, `build_sample_starfield_with_guards` ~line 379)
- **Fix present**: Yes
- **Guard test**: `tests::parses_a_handbuilt_skyrim_be_pex` and `tests::parses_a_handbuilt_starfield_pex_with_guards` in `crates/pex/src/lib.rs` — both pass
- **Notes**: Tests are hand-built-binary decode-fidelity tests rather than a literal write-then-read round trip, but exercise exactly the BE-Skyrim and Starfield-guards decode arms the issue flagged as uncovered.

## #1740: SCR-D5-03: no decompiled-.pex parity test for DA10
- **Status**: PASS
- **Closed**: 2026-07-03
- **Fix commit**: `2f0b99fa` — "add a DA10 .pex byte-equality parity test"
- **Fix site**: `crates/scripting/tests/pex_recognize_e2e.rs` (`da10_pex_reproduces_hand_builder_byte_for_byte`, line 81)
- **Fix present**: Yes
- **Guard test**: `da10_pex_reproduces_hand_builder_byte_for_byte` in `crates/scripting/tests/pex_recognize_e2e.rs` — present, `#[ignore]`-gated (requires Skyrim SE game data); not executed this pass
- **Notes**: Present and correctly located; opt-in/`#[ignore]`d like every other real-content test in this codebase — a deliberate, documented convention, not a gap.

## #1731: LC-D7-02: VWD / "Has Distant LOD" record-header flag (0x00010000) not parsed
- **Status**: PASS
- **Closed**: 2026-07-03
- **Fix commit**: `175ebf2c` — "parse and expose the VWD record-header flag"
- **Fix site**: `crates/plugin/src/esm/reader.rs` (`FLAG_VISIBLE_WHEN_DISTANT: u32 = 0x00010000` at line 27, `RecordHeader::is_visible_when_distant()` at line 384)
- **Fix present**: Yes
- **Guard test**: `esm::reader::tests::vwd_flag_is_surfaced_when_set` and `vwd_flag_is_false_when_unset` in `crates/plugin/src/esm/reader.rs` — both pass
- **Notes**: Follow-on #1889 (materialize VWD as a per-placement marker component) also present in history but out of scope for this issue's own acceptance criteria.

## #1718: FNV-D7-01: Ragdoll body + dependent constraints dropped silently on bone-name miss (no telemetry)
- **Status**: PASS
- **Closed**: 2026-07-03
- **Fix commit**: `ffe9a816` — "log dropped ragdoll bodies/constraints on bone-name miss"
- **Fix site**: `byroredux/src/ragdoll.rs` (`template_from_imported`, `log::warn!` at dropped-body site line ~111 and dropped-constraint site line ~144)
- **Fix present**: Yes
- **Guard test**: `ragdoll::tests::dropped_bone_excludes_body_and_dependent_constraint_but_keeps_the_rest` in `byroredux/src/ragdoll.rs` — passes
- **Notes**: Test pins the functional drop/remap behavior the `log::warn!` calls are attached to, not the log output itself — matches an established codebase convention (see also #1539).

## #1823 (supersedes #1651): BGSM/BGEM blend-factor revert
- **Status**: PASS — confirmed the #1823 revert holds; #1651's swap has NOT crept back in
- **Closed**: 2026-07-02
- **Fix commit**: `27334481` — "remove wrong 0/1 blend-factor swap that corrupted FO4 Additive/Multiplicative materials"
- **Fix site**: `byroredux/src/asset_provider/material.rs` — the function formerly named `gl_to_gamebryo_blend` (the #1651 swap) is now `bgsm_blend_to_gamebryo(raw: u32) -> u8`, a plain narrowing cast (`raw as u8`) with **no 0↔1 swap**. Doc comment explicitly records the premise-was-false history and warns against restoring the swap.
- **Fix present**: Yes — no swap logic present anywhere in the current blend-factor path
- **Guard test**: `bgsm_blend_to_gamebryo_is_identity_narrowing` and `bgsm_merge_forwards_alpha_blend_mode` in `byroredux/src/asset_provider/tests.rs` — not independently re-run this pass, but covered by the general `cargo test -p byroredux` sweep implied by other guard tests above against the same binary target
- **Notes**: Correct present-day state. No regression of the #1823 revert detected.

**Batch E summary: 12 PASS, 1 PARTIAL, 0 FAIL, 0 UNVERIFIABLE.**

## Step 4 — Unconditional Fragile-Area Checks

## Fragile-area check: NIFAL single material boundary
- **Status**: PASS
- **Evidence**: `byroredux/src/material_translate.rs:73-77` — `translate_material` remains the sole `ImportedMesh → Material` translation site. All production `Material {` struct-literal construction sites repo-wide resolve to `material_translate.rs:78` (the boundary) plus `byroredux/src/cornell.rs` (synthetic Cornell-box test-harness materials, out of scope). `crates/core/src/ecs/components/material.rs` — `Material::metalness` (line 217) and `Material::roughness` (line 223) remain plain `f32`, not `Option<f32>`. `Material::resolve_pbr` (line 686) is the only call site of `classify_pbr_keyword`, guarded on NaN-sentinel unresolved slots. Grep of `crates/renderer/shaders/` for game-name/keyword classification branching returned zero hits.
- **Notes**: None.

## Fragile-area check: NIFAL typed particle emitters
- **Status**: PASS
- **Evidence**: `crates/nif/src/blocks/particle.rs` defines `NiPSysEmitter` (176), `NiPSysEmitterCtlr` (187), `NiPSysEmitterCtlrData` (195), `NiPSysGrowFadeModifier` (209) as typed structs with dedicated parsers. `crates/nif/src/blocks/mod.rs` dispatch table routes all four by name to those parsers — none fall into an opaque `NiPSysBlock` catch-all. `crates/nif/src/import/walk/mod.rs:687` `extract_emitter_params` and `:786` `extract_emitter_rate` produce `ImportedEmitterParams`, wired through to `byroredux/src/systems/particle.rs:29` `apply_emitter_params`.
- **Notes**: None.

## Fragile-area check: NIFAL collision shape coverage
- **Status**: PASS
- **Evidence**: `BhkMultiSphereShape` and `BhkConvexListShape` both translate to `CollisionShape::Ball`/`CollisionShape::Compound` (not unconditional `None`) in `crates/nif/src/import/collision/shape.rs:110-136` and `:228-243`. Only empty post-filter/resolved-set residue falls through to `None` — a legitimate degenerate-input fallback, not a blanket drop.
- **Notes**: The shape-resolution logic lives in `crates/nif/src/import/collision/shape.rs`, not `mod.rs` — moved by commit `41152f13` (#1876, module split). File-layout drift only, not a regression; matches the established Session 34/35 module-split pattern.

## Fragile-area check: Disney BSDF / ReSTIR
- **Status**: PASS
- **Evidence**: `crates/renderer/shaders/include/pbr.glsl` exists, containing the GGX/anisotropic-GGX/Smith/Fresnel/Disney-diffuse-split lobe; `crates/renderer/shaders/triangle.frag:10-19` carries the GLSL-PathTracer MIT attribution + Burley citation block. Grep for `resRadiance` across `crates/renderer/shaders/` returns only two comment references confirming retirement (`include/lighting.glsl:64`, `triangle.frag:2006`) — no live array declaration or G-buffer attachment. `shadowableLightRadiance()` (`include/lighting.glsl:72`) is called from multiple ReSTIR sites in `triangle.frag` (lines 2216, 2385, 2497, 2592, 2762), recomputing unshadowed radiance per-use rather than reading a stored per-reservoir array.
- **Notes**: None.

## Fragile-area check: GPU struct size-pin tests
- **Status**: PASS
- **Evidence**: `cargo test -p byroredux-renderer gpu_` → 25 passed, 0 failed, including `gpu_instance_is_112_bytes_std430_compatible` and `gpu_camera_is_336_bytes` in `crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs`.
- **Notes**: None.

**Step 4 summary: 5/5 PASS, no regressions found.**

## Summary Table

| Issue | Title | Status | Fix Present | Guard |
|-------|-------|--------|-------------|-------|
| #1882 | BSWeakReferenceNode Starfield tail | PASS | Yes | passes |
| #1883 | Per-block coverage gate blind spot | PARTIAL | Yes (1 of 3) | passes |
| #1885 | interpolator.rs allocate_vec | PASS | Yes | passes |
| #1887 | XATO REFR FONV-only comment | PARTIAL | Yes (comment only) | none |
| #1896 | FO76 particle block field drops | PASS | Yes | passes |
| #1898 | Max Filepath magic-literal rename | PARTIAL | Yes | none (untestable) |
| #1899 | Oblivion TSV baseline stale-high | PASS | Yes | passes |
| #1900 | Starfield clean-rate floors | PASS | Yes | passes |
| #1901 | FO4 backlight-power threshold | PARTIAL | Yes | none (gap case) |
| #1902 | BhkMultiSphereShape bulk read | PASS | Yes | passes |
| #1903 | read_sized_string alloc bound | PASS | Yes | passes |
| #1904 | Renderer FFI unsafe SAFETY comments | PASS | Yes | clippy deny lint |
| #1905 | quest_stage_gate multi-quest decline | PASS | Yes | passes |
| #1906 | Papyrus literal leading-minus | PASS | Yes | passes |
| #1907 | lower_fragment side-effect decline | PASS | Yes | passes |
| #1908 | Out-of-range Papyrus literals | PASS | Yes | passes |
| #1909 | rumble recognizer non-literal decline | PASS | Yes | passes |
| #1913 | SHADOW_MASK_* 8-bit ceiling pin | PASS | Yes | passes |
| #1916 | GpuLight 4-copy lockstep enumeration | PASS | Yes | passes |
| #1917 | composite.frag.spv stale recompile | PASS | Yes | passes |
| #1920 | generated_header defines value-pins | PASS | Yes | passes |
| #1921 | Staging buffer release size | PARTIAL | Yes | none |
| #1925 | "scrap" classifier narrowed | PASS | Yes | passes |
| #1926 | Composite fog fallback dead code | PASS | Yes | passes |
| #1927 | XCLL cubic-fog unreachable | PARTIAL | Yes (doc only) | none |
| #1929 | triangle.vert.spv SPIR-V version | PASS | Yes | passes |
| #1932 | TAA Halton jitter taa_failed gate | PASS | Yes | passes |
| #1934 | caustic_splat.comp macro coverage | PASS | Yes | passes |
| #1937 | Volumetrics sun-hemisphere fix | PARTIAL | Yes | none (no cargo surface) |
| #1939 | Effect_Lit sun-hemisphere fix | PARTIAL | Yes | none (no cargo surface) |
| #1979 | Ragdoll non-simulated descendants | PASS | Yes | passes |
| #1980 | CycleType::Reverse clamp | PASS | Yes | passes |
| #1985 | FO4 alpha-test threshold seed | PASS | Yes | passes |
| #1986 | CSG non-final chunk length pin | PASS | Yes | passes |
| #1994 | Additive-blend sort key ordering | PASS | Yes | passes |
| #1996 | parse_npc FormID remap | PASS | Yes | passes |
| #1815 | Decompiler recursion-depth cap | PASS | Yes | passes |
| #1816 | translate_pex catch_unwind | PARTIAL | Yes | none |
| #1728 | Skyrim-BE/Starfield round-trip tests | PASS | Yes | passes |
| #1740 | DA10 .pex byte-equality parity test | PASS | Yes | present, not run (`#[ignore]`) |
| #1731 | VWD record-header flag parse | PASS | Yes | passes |
| #1718 | Ragdoll bone-miss telemetry | PASS | Yes | passes |
| #1823 (⊃#1651) | BGSM/BGEM blend-factor revert | PASS | Yes | present (not re-run) |

**Total: 43 issues verified — 33 PASS, 10 PARTIAL, 0 FAIL, 0 UNVERIFIABLE.**
**Plus Step 4: 5/5 fragile-area checks PASS.**

## Regressions Found

**None.** Every previously-closed fix's code change is present and intact in the live tree. No `FAIL` status was assigned to any of the 43 issues checked or the 5 Step 4 fragile-area contracts.

## Process Notes (not regressions, but worth follow-up)

1. **#1883 premature closure**: closed as if all three sub-findings (NIF-D3-001/002/003) were resolved, but only NIF-D3-002 (union-key histogram iteration) shipped. NIF-D3-001 (alias-collapse of ~28 `NiPSys*Modifier` types into one `NiPSysBlock` bucket) and NIF-D3-003 (single-archive coverage scope) remain open in the live tree per the fix commit's own deferral note. Recommend either reopening a tracking issue for the two deferred items or explicitly documenting them as accepted scope in `docs/`.
2. **Guard-test gaps** (fix correct, no automated regression pin): #1921 (staging buffer capacity-vs-upload-size), #1901 (FO4 backlight-power `[3.0e38, FLT_MAX)` boundary), #1816 (`translate_pex` `catch_unwind`), #1937/#1939 (sun-hemisphere shader fixes — no cargo-testable surface, RenderDoc-only). None of these show any sign of having regressed; they are flagged purely as hardening opportunities for a future session.

Suggested follow-up: `/audit-publish docs/audits/AUDIT_REGRESSION_2026-07-16.md` (though note there are no NEW findings to publish — this report is a clean bill of health with process-note follow-ups only).
