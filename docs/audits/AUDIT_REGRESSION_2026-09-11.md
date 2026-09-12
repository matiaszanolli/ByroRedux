# Regression Verification Audit — 2026-09-11

Scope: default discovery (`--label bug`, `--limit 50`) via
`gh issue list --repo matiaszanolli/ByroRedux --state closed --label bug --limit 50`,
plus the unconditional Step 4 fragile-area checks from the `audit-regression`
skill. The discovery window returned 50 closed `bug`-labeled issues, all closed
2026-09-08 through 2026-09-11 (numbers #3999–#4106) — these are almost entirely
prior audit findings (`ECS-`, `ESM-`, `REN-`, `CHAR-`, `INC-` prefixes) filed via
`/audit-publish` and then fixed, rather than user-reported bugs. The repo has
3900+ closed issues total; the default `--limit 50` covers only the most recent
window (see the skill's discovery-window caveat) — no `--issues` pass was run
beyond this default, per the task instruction to use default parameters.

Verification was split five ways across parallel sub-agents (10 issues each,
by number range: #4091–#4106, #4076–#4090, #4061–#4074, #4011–#4060,
#3999–#4010), each of which fetched the issue body, located the fix commit,
re-read the live code at the cited symbol, located/ran the guard test, and
assigned PASS/PARTIAL/FAIL/UNVERIFIABLE. All five batches have reported back
in full. Results are merged below.

**Bottom line: 45 PASS, 5 PARTIAL, 0 FAIL, 0 UNVERIFIABLE. No regressions
found in any of the 50 verified issues.** Three additional findings (all LOW)
surfaced incidentally during verification and Step 4 — see Findings section.

## Step 4 — Unconditional fragile-area checks (run directly, not delegated)

- **NIFAL single material boundary.** `byroredux/src/material_translate.rs:475`
  (`translate_material`) remains the only `ImportedMesh → Material` site; no
  competing `-> Material` constructor exists outside test helpers
  (`cornell.rs`, `helpers.rs` test fixtures). `Material::metalness` /
  `Material::roughness` (`crates/core/src/ecs/components/material.rs:400,406`)
  are still plain resolved `f32` fields, no `Option<f32>`. **PASS.**
- **Typed particle emitters.** `NiPSysEmitter` / `NiPSysEmitterCtlr` /
  `NiPSysEmitterCtlrData` / `NiPSysGrowFadeModifier` still parse as typed
  structs in `crates/nif/src/blocks/particle.rs` and are dispatched by name in
  `crates/nif/src/blocks/mod.rs` (lines 1073, 1097, 1165, plus the sibling
  `NiPSysEmitter*Ctlr` variant arms at 1170–1181). `extract_emitter_params` /
  `extract_emitter_rate` now live in `crates/nif/src/import/walk/emitter.rs`
  (split out of `walk/mod.rs` since the doc was last written — path drift, not
  a functional regression) and still populate `ImportedEmitterParams`
  (`crates/nif/src/import/types.rs:1795`), consumed by `apply_emitter_params`
  (`byroredux/src/systems/particle.rs:29`). **PASS.**
- **Collision shape coverage.** `BhkMultiSphereShape` (shape.rs:110) and
  `BhkConvexListShape` (shape.rs:243) both still resolve to a `CollisionShape`
  in `crates/nif/src/import/collision/shape.rs` rather than dropping to
  `None`. **PASS.**
- **Disney BSDF / ReSTIR reservoir retirement.** No live `resRadiance[...]`
  array remains anywhere in `crates/renderer/shaders/` — the only two hits are
  historical comments (`lighting.glsl:167`, `triangle.frag:2970`) documenting
  its removal. `shadowableLightRadiance` (`include/lighting.glsl:174`) is
  present and is the WRS unshadowed-radiance recompute path. The Disney/Burley
  lobe lives in `crates/renderer/shaders/include/pbr.glsl` with the
  GLSL-PathTracer MIT attribution intact in `triangle.frag` (lines 20–31).
  **PASS.**
- **`#[repr(C)]` GPU struct size pins.** `cargo test -p byroredux-renderer gpu_`
  → **53 passed, 0 failed.** `gpu_instance_is_160_bytes_std430_compatible` and
  `gpu_camera_is_368_bytes` both hold as documented. One doc-drift noted below
  (REG-01, not a regression).

## Per-issue results

### Batch A — #4091–#4106 (CHAR audit findings, closed 2026-09-11)

| Issue | Title (short) | Status | Fix commit | Guard test |
|---|---|---|---|---|
| #4091 | `stamp_creature_attack` bypassed TPLT resolution | PASS | `6bcd1666` | `templated_creature_gets_attack_damage_from_the_use_stats_template` |
| #4092 | spawn tail ignored `Use Traits` race for mesh resolution | PASS | `6bcd1666` | `prebaked_equip_state_uses_the_passed_race_form_id_not_the_shells_own` |
| #4093 | `Use Factions`/`Use AI Packages` never resolved | PASS | `6bcd1666` | `templated_npc_gets_faction_ranks_from_the_use_factions_template`, `templated_npc_runs_the_use_ai_packages_templates_behavior` |
| #4094 | no test coverage for FO76/Starfield `Stored` AVIF resolution | PASS | `64f586a7` | `fo76_stored_avif_outputs_resolve_on_shipped_master`, `starfield_stored_avif_outputs_resolve_on_shipped_master` (ran `--ignored` against real ESMs) |
| #4095 | `probe_combat_fixture` re-derived actor level with rejected `.max(1)` clamp | PARTIAL | `64f586a7` | none — dev-tool example binary, no test feasible |
| #4096 | `DetectorState` multiplier assignment unsourced | PASS | `855d10cd` | `detector_skill_and_state_match_the_source_table` |
| #4103 | `ActiveAffliction.band` stored an unstable raw index | PASS | `91528dc6` | `reevaluate_survives_the_table_being_reordered_between_ticks` |
| #4104 | `pool_regen_tick_system` used hardcoded `level = 1` | PASS | `91528dc6` | `magicka_regen_uses_the_entitys_own_character_level` |
| #4105 | overstated `CreatureAttack` fix claims | PARTIAL | `9d792226` | none — doc/comment wording correction, no test possible |
| #4106 | no test for `derive_skyrim_actor_values` per-pool independence | PASS | `64f586a7` | `skyrim_pools_resolve_independently_when_one_is_missing` |

Batch A: 8 PASS, 2 PARTIAL, 0 FAIL, 0 UNVERIFIABLE. Both PARTIALs are
self-evidently untestable (a dev-tool example binary; a prose-only wording
fix) rather than rigor gaps — no finding filed for either.

### Batch B — #4076–#4090 (ESM/plugin + boot-gate findings, closed 2026-09-10/11)

| Issue | Title (short) | Status | Fix commit | Guard test |
|---|---|---|---|---|
| #4076 | `parse_wrld_group` world-children `sub_end` unclamped to parent | PASS | `6c3584ec` | `world_children_group_cannot_overrun_its_top_level_parent` |
| #4077 | `6 if current_cell.is_none()` arm rests on a false premise | PASS | `6c3584ec` | wrld fixture test updated, passes |
| #4078 | `ATXT` with no following `VTXT` silently drops the terrain texture layer | PASS | `f1b39168` | `atxt_immediately_followed_by_another_atxt_flushes_the_first_with_no_alpha`, `atxt_as_the_final_sub_record_still_flushes` |
| #4079 | DIAL topic-children parent check compares raw GRUP label vs remapped FormID | PASS | `f1b39168` | `topic_children_label_is_remapped_before_dial_lookup` |
| #4080 | stale 3,887-line `parse_real_esm.rs.orig` merge artifact tracked | PASS | `5f644b50` | none applicable (file removal); `.gitignore` `*.orig`/`*.rej` rule is the recurrence guard |
| #4081 | `LegacyFormId::is_null` tests 24-bit standard local id for ESL/ESH forms | PASS | `5f644b50` | `is_null_dispatches_on_slot_kind_for_esl_and_esh` |
| #4082 | `resolve_winner` breaks equal-depth ties by slice order, mislabels as `DepthResolved` | PASS | `5f644b50` | `diamond_dependency_ties_surface_as_tiebreak_not_depth_resolved` |
| #4087 | boot `SOURCES` completeness gate walks two hard-coded directories | PASS | `c6328372` | `the_concat_list_covers_every_file_in_the_boot_directory` |
| #4088 | `TRUNCATING_TEST_MODULES` hand-maintained with no completeness gate | PASS | `c6328372` | `every_schedule_mod_test_module_is_a_sentinel_or_a_documented_exemption`, `every_production_file_precedes_the_first_test_module` |
| #4090 | `cargo clippy` red at HEAD (core `--workspace -D warnings`; renderer `--all-targets`) | PASS | `da3d5f22` | CI command itself, re-run directly and confirmed green for the exact scope #4090 fixed |

Batch B: 10 PASS, 0 PARTIAL, 0 FAIL, 0 UNVERIFIABLE. One incidental finding
surfaced while re-verifying #4090 — see REG-02 below (not a regression of
#4090; a stricter, never-fixed lint combination).

### Batch C — #4061–#4074 (ECS + ESM FormID-remap findings, closed 2026-09-08/10/11)

| Issue | Title (short) | Status | Fix commit | Guard test |
|---|---|---|---|---|
| #4061 | roots-cache key blind to a Parent remove/insert pair | PASS | `ffee9e74` | `a_parent_remove_and_insert_that_nets_out_still_rebuilds_the_root_set` |
| #4062 | transform-propagation BFS marks every parent dirty just to read it | PASS | `ffee9e74` | `a_read_only_parent_lookup_does_not_dirty_the_parent` |
| #4063 | six of seven M42 AI procedures invisible to byro-dbg | PASS | `9ca04e06` | `every_ai_procedure_behavior_component_is_registered` |
| #4064 | declaration-completeness test covers only load-bearing-free systems | PASS | `9823ba2f` | `every_parallel_system_declares_everything_it_acquires`, `the_parallel_system_table_covers_every_parallel_registration` |
| #4066 | `CLMT.WLST` weather FormIDs never remapped | PASS | `f5a576fb` | `parse_clmt_wlst_remaps_to_global_form_id_space`, `parse_clmt_wlst_without_a_remap_is_identity` |
| #4067 | `CommonNamedFields` stores `SCRI` raw, mixed FormID spaces | PASS | `e962ec96` | `parse_acti_remaps_scri_to_global_space` |
| #4069 | PERK.EPFD / WTHR / SCPT read FormIDs outside remap | PARTIAL | `48acaea2` | general allowlist tests pass; `SCRO` has a dedicated test, MNAM/NNAM and the EPFD arm do not |
| #4070 | `parse_mgef` remaps only 1 of 3 embedded FormIDs | PARTIAL | `48acaea2` | general allowlist tests pass; no field-specific test for `associated_item`/`effect_shader_id` |
| #4071 | `parse_spel`/`parse_ench`/`parse_mesg` read FormIDs with no remap param | PARTIAL | `48acaea2` | general allowlist tests pass; no field-specific test for `EFID`/`QNAM` |
| #4074 | `main_body_bit`'s FO76/Starfield arms unsourced | PASS | `51b95787` | `fo76_and_starfield_arms_are_marked_provisional`, `fo76_and_starfield_currently_follow_fo4` |

Batch C: 7 PASS, 3 PARTIAL, 0 FAIL, 0 UNVERIFIABLE. #4069/#4070/#4071 share
one fix commit and one residual gap shape — merged into a single finding,
REG-03 below.

### Batch D — #4011–#4060 (Renderer + ECS findings, closed 2026-09-08/09)

| Issue | Title (short) | Status | Fix commit | Guard test |
|---|---|---|---|---|
| #4011 | `screen_scaled_reservation_bytes` covers only the caustics glass half | PASS | `28c87cb5` (closed as dup of #3992's fix) | `caustic_bytes_per_pixel_matches_documented_memory_budget` |
| #4016 | terrain-splat normal-map loop takes implicit-LOD sample under `continue` | PASS | `e47d486c` | `every_terrain_splat_sampler_uses_explicit_gradients`, `the_perturb_normal_wrapper_only_delegates` |
| #4017 | `giHitIrradiance` dead shader code since 2026-07-29 | PASS | `e47d486c` | `the_gi_visible_light_cap_is_wired_to_the_live_path_budget` |
| #4018 | unsafe-vertex-lane guard covers only 2 of 4 shaders reading raw vertex SSBO | PASS | `e47d486c` | `rt_hit_shaders_have_no_unsafe_vertex_data_reads` (rewritten to a discovery walk) |
| #4027 | terrain-tile shift/mask hand-written shader-side with no lockstep pin | PASS | `f98e2a44` | `instance_terrain_tile_bits_match_scene_buffer_consts` |
| #4034 | image-health-docs guard scans `draw.rs` for a call site that moved | PASS | `d9a7e75c` | `image_health_docs_no_longer_claim_fence_alone_proves_host_visibility` |
| #4035 | bloom-upload guard scans `draw.rs` but the section moved | PASS | `d9a7e75c` | `draw_frame_does_not_re_upload_bloom_params_every_frame` |
| #4036 | `signal_temporal_discontinuity`'s `previous_rigid_models.clear()` inert at all 3 call sites | PASS | resolved by `ea6d2437` (Fix #4007), cross-closed | `rigid_history_lookup_is_gated_on_the_one_shot_suppression_latch` |
| #4038 | `pending_destroy_*` accessors have zero call sites | PASS | resolved by `3a221d8e` (Fix #3999), cross-closed | `every_blas_residency_accessor_reaches_the_rt_integrity_snapshot` |
| #4060 | clean node between two dirty entities strands the subtree below | PASS | `96dc4093` | `a_clean_node_between_two_dirty_ones_does_not_strand_the_subtree_below`, `dirty_descendant_is_recomposed_after_a_later_dirty_ancestor` |

Batch D: 10 PASS, 0 PARTIAL, 0 FAIL, 0 UNVERIFIABLE. #4060 was the one
HIGH-severity issue in this batch (transform-propagation correctness) and
holds cleanly.

### Batch E — #3999–#4010 (Renderer/Vulkan findings, closed 2026-09-08)

| Issue | Title (short) | Status | Fix commit | Guard test |
|---|---|---|---|---|
| #3999 | four `pub` accessors on `AccelerationManager` have zero call sites | PASS | `3a221d8e` | `every_blas_residency_accessor_reaches_the_rt_integrity_snapshot` |
| #4000 | `AccelerationManager::skinned_blas` was the last std `HashMap` on the per-frame skinned path | PASS | `0a796eef` | `skinned_blas_stays_fx_hashed` |
| #4001 | `drop_skinned_blas` bypassed `DEFAULT_COUNTDOWN` | PASS | `9944709f` | `no_deferred_destroy_push_reaches_around_the_shared_countdown` |
| #4002 | `dof_effective_view_proj` applied lens jitter in absolute space | PASS | `c512ba35` | `the_aperture_jitter_is_render_relative_not_absolute` |
| #4003 | `depth.stats` looped forever with no console-visible reason on non-D32_SFLOAT devices | PASS | `17949a9b` | `an_uncapturable_depth_format_is_reported_instead_of_re_arming_forever`, `an_uncapturable_device_is_never_asked_for_a_capture` |
| #4004 | pipeline-cache header gate accepted `headerSize` larger than the file | PASS | `2cbf8b5a` | `header_size_larger_than_the_file_returns_false`, `header_size_may_equal_but_not_exceed_the_file_length`, `larger_header_size_returns_true_when_other_fields_match` |
| #4006 | TAA-failure recovery could rewrite a possibly-pending descriptor set | PASS | `803d2d0e` | `the_taa_failure_fallback_defers_its_descriptor_rebind_past_the_fence_wait` |
| #4007 | `signal_temporal_discontinuity` had phase-dependent semantics | PASS | `ea6d2437` | `both_in_frame_limbs_stay_wired_to_their_order_independent_form` |
| #4008 | five-copy `octDecode` duplication: discoverability fixed, drift not | PASS | `ea6d2437` | `octahedral_codec_has_a_single_definition_every_consumer_includes` |
| #4010 | `water.frag` caustic refraction hardcoded `1.0/1.33` instead of authored IOR | PASS | `ea6d2437` | `water_caustic_refraction_uses_the_authored_ior_not_the_1_33_default` |

Batch E: 10 PASS, 0 PARTIAL, 0 FAIL, 0 UNVERIFIABLE. All 11 guard tests
across these 10 issues ran green via `cargo test -p byroredux-renderer` /
`cargo test -p byroredux --bin byroredux`; none needed a live Vulkan device
since every guard is a source-scan or pure-function unit test.

## Summary table

| Batch | Issues | PASS | PARTIAL | FAIL | UNVERIFIABLE |
|---|---|---|---|---|---|
| A | #4091–#4106 (10) | 8 | 2 | 0 | 0 |
| B | #4076–#4090 (10) | 10 | 0 | 0 | 0 |
| C | #4061–#4074 (10) | 7 | 3 | 0 | 0 |
| D | #4011–#4060 (10) | 10 | 0 | 0 | 0 |
| E | #3999–#4010 (10) | 10 | 0 | 0 | 0 |
| **Total** | **50** | **45** | **5** | **0** | **0** |

**No regressions found.** Every one of the 50 discovered closed-bug fixes is
still present and correct in the current tree; 45 have a guard test that runs
green, and the remaining 5 PARTIALs are fix-present-but-guard-thin (three of
them, #4069/#4070/#4071, share a genuine residual hardening gap — see REG-03;
the other two, #4095/#4105, are dev-tool/prose-only sites where no test is
meaningfully possible).

## Findings

### REG-01: `_audit-common.md` GpuMaterial size reference is stale (432 B vs live 428 B)
- **Severity**: LOW
- **Dimension**: Regression / doc-rot
- **Location**: `.claude/commands/_audit-common.md:101`
- **Status**: NEW
- **Description**: The shared audit doc states GpuMaterial's "current size is pinned by `gpu_material_size_is_432_bytes`, not `_348_`" — but the live, passing test is `gpu_material_size_is_428_bytes` (`crates/renderer/src/vulkan/material.rs`). The shrink is deliberate and already fully reconciled in code: `a65dbffe` ("Fix #3909: remove GpuMaterial.texture_index, the unsampled lane in the dedup key") dropped the struct from 432 → 428 B and updated the test name, the offset pin, the GLSL field-name needle list, `bindings.glsl`, and the struct's own doc comments all in the same commit. Only this one shared-skill-doc line was left unsynced.
- **Evidence**: `cargo test -p byroredux-renderer gpu_` → `gpu_material_size_is_428_bytes ... ok` (53 passed total); `git log -p -S"432" -- crates/renderer/src/vulkan/material.rs` shows `a65dbffe` rewriting every in-code reference from 432→428 in lockstep, leaving no other stale reference in the crate.
- **Impact**: None on shipped code — this is a documentation pointer used by future audits, not a code contract. A future auditor trusting this line would look for a nonexistent `_432_` test and could misreport a false regression.
- **Related**: #3909 (the actual GpuMaterial shrink fix).
- **Suggested Fix**: Update `_audit-common.md:101` to cite `gpu_material_size_is_428_bytes` and 428 B.

### REG-02: `cargo clippy --workspace --all-targets -- -D warnings` is red at HEAD on `field_reassign_with_default`, a lint #4090 never covered
- **Severity**: LOW
- **Dimension**: tech-debt / CI-gate
- **Location**: test code across `crates/core`, `crates/renderer`, and the `byroredux` binary (surfaced by re-running the CI clippy command during #4090 verification; specific file list not yet enumerated)
- **Status**: NEW
- **Description**: #4090's fix (`da3d5f22`) made the *documented* clippy invocations green: `cargo clippy --workspace -- -D warnings` (core's actual CI command) and `cargo clippy -p byroredux-renderer --all-targets` (without `-D warnings`). Both are still green, confirmed by re-running them directly. However, the *combined* stricter invocation `cargo clippy --workspace --all-targets -- -D warnings` fails today on `clippy::field_reassign_with_default`, a different lint from every one #4090 addressed. This was never claimed fixed by #4090's commit and is not a regression of it — it's a pre-existing gap in test code that no CI job currently exercises with that exact flag combination.
- **Evidence**: Sub-agent verifying #4090 independently ran `cargo clippy --workspace --all-targets -- -D warnings` and reported failures on `clippy::field_reassign_with_default` across core/renderer/binary test code, while the two commands #4090's fix actually targets remain clean.
- **Impact**: Low — cosmetic lint in test code, not a correctness issue. But it is the same class of gap the project's own `CLAUDE.md` calls out for `cargo test -p byroredux-core` (a documented command that silently omits coverage): here a plausible "make CI stricter" step would immediately go red on unrelated findings, encouraging either a rushed mass-fix or reverting the stricter flag.
- **Related**: #4090 (adjacent, not overlapping scope).
- **Suggested Fix**: File a follow-up incremental/tech-debt issue enumerating the `field_reassign_with_default` sites and either fix them or explicitly scope CI's clippy command to exclude `--all-targets` with a documented reason.

### REG-03: FormID-remap fixes for #4069/#4070/#4071 lack field-level regression tests for several individually-remapped fields
- **Severity**: LOW
- **Dimension**: esm-plugin / test-gap
- **Location**: `crates/plugin/src/esm/records/weather.rs` (MNAM/NNAM), `crates/plugin/src/esm/records/misc/magic.rs` (`parse_perk` EPFD function-type-4 arm; `parse_mgef`'s `associated_item`/`effect_shader_id`; `MagicEffectAccumulator::feed`'s `EFID`), `crates/plugin/src/esm/records/misc/dialogue.rs` (`parse_mesg`'s `QNAM`)
- **Status**: NEW
- **Description**: `48acaea2` (Fix #4069, #4070, #4071) correctly wraps all the FormID fields named in these three issues in `remap_fid`, and the general regression suite (`every_record_parser_takes_a_remap_or_is_explicitly_exempt`, `parsers_that_take_a_remap_actually_use_it` in `crates/plugin/src/esm/records/tests.rs`) confirms every parser that declares a `remap` parameter still uses it somewhere in its body. But that suite is a *signature/usage* scan, not a *per-field* scan: it would catch a parser losing its `remap` parameter outright, but not a narrower regression where one specific field's `remap_fid()` call is reverted to a raw read while the parser still uses `remap` elsewhere. Of the six touched fields, only `SCRO` (`parse_scpt_remaps_scro_but_never_scrv`, `script.rs`) and `WLST` (from the sibling #4066 fix) have a dedicated non-identity-remap test; `MNAM`/`NNAM`, the EPFD FormId arm, `associated_item`/`effect_shader_id`, `EFID`, and `QNAM` are all currently exercised only under `&None` (identity) in existing tests.
- **Evidence**: `grep -n "remap_fid" crates/plugin/src/esm/records/weather.rs crates/plugin/src/esm/records/misc/magic.rs crates/plugin/src/esm/records/misc/dialogue.rs` confirms all six sites are wrapped; cross-checking each field's test coverage found only identity-remap (`&None`) assertions for the five fields named above (three independent sub-agents converged on this same gap while verifying #4069/#4070/#4071 separately).
- **Impact**: Test-coverage gap only; no shipped wrong value today — code correctly remaps all six fields right now. But a future refactor could silently un-wrap any one of the five uncovered fields (e.g. during another `misc/magic.rs` split) and no test in the suite would catch it, since the general allowlist test only asserts *some* use of `remap`, not *this specific field's* use of it.
- **Related**: #4069, #4070, #4071 (the three issues this gap spans); #4066 (the sibling fix that DID get a field-level test, for comparison).
- **Suggested Fix**: Add one non-identity-remap assertion per currently-uncovered field, following the existing `parse_clmt_wlst_remaps_to_global_form_id_space` / `parse_scpt_remaps_scro_but_never_scrv` pattern (construct a `FormIdRemap` that maps a known local FormID to a distinguishable global one, assert the parsed field carries the remapped value, not the raw one).

## Suggested next step

```
/audit-publish docs/audits/AUDIT_REGRESSION_2026-09-11.md
```
