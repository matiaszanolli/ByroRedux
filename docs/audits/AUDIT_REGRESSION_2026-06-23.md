# Regression Verification Audit — 2026-06-23

Confirms that recently-closed bug fixes are still present in the live tree and
still guarded by a green test. Scope: the Session 49–51 fix wave called out in
`SKILL.md` (#1590, #1592, #1594, #1606, #1650, #1651, #1652, #1656, #1658) plus
the unconditional Step-4 fragile-area contracts (NIFAL boundary, typed particle
emitters, collision-shape coverage, Disney/BSDF + reservoir removal, `#[repr(C)]`
GPU struct pins).

**Method**: for each issue — locate the fix commit (`git log --grep="#N"`),
confirm the fix code at the named symbol in the live tree (anchored on symbol
names, not line numbers), locate the guard test, and run it green. Step-4
contracts checked independently of GitHub discovery.

## Result

**0 regressions.** All 9 priority fixes are PASS (code present + guard test
green). All Step-4 fragile-area contracts hold. No findings to publish.

| Issue | Title (short) | Status | Fix Present | Guard |
|-------|---------------|--------|-------------|-------|
| #1590 | FO4 DLC precombine resolved by owning plugin | PASS | Yes | `oc_nif_path_dlc_uses_subdir_and_zeroes_mod_byte`, `oc_nif_path_base_game_stays_at_root`, `resolve_precombine_owner_follows_form_id_mod_index` — green |
| #1592 | FO4 NIF model-space-normals + alpha-test flags consumed | PASS | Yes | `fo4_model_space_normals_flag_sets_field`, `fo4_alpha_test_flag_sets_field`, `fo76_crc_model_space_normals_sets_field`, `skyrim_bsver_does_not_read_fo4_f2_alpha_test`, `fo4_no_flags_leaves_defaults` — green |
| #1594 | FO4 BSConnectPoint attach graph materialized onto entities | PASS | Yes | `stamp_attach_components_materializes_onto_root_entity`, `attach_points_component_interns_and_maps_parent_bone`, `child_connections_resolve_against_parent_attach_points` — green |
| #1606 | Starfield BSLightingShaderProperty trailing field captured | PASS | Yes | `parse_bs_lighting_starfield_tail_empty_without_size_or_drift` — green |
| #1650 | Oblivion 16-byte ACBS parsed (level + gender) | PASS | Yes | `oblivion_16byte_acbs_parses_level_and_gender`, `fnv_ignores_16byte_acbs` — green |
| #1651 | BGSM/BGEM blend factors GL→Gamebryo translated | PASS | Yes | `gl_to_gamebryo_blend_swaps_only_zero_and_one`, `bgsm_merge_forwards_alpha_blend_mode` — green |
| #1652 | bhk motion_type → MotionType via full Havok enum | PASS | Yes | `havok_motion_type_maps_full_enum` — green |
| #1656 | Unload-walk test covers ExtraTextureMaps (6 slots) | PASS | Yes | `unload_walk_collects_all_texture_handle_components` (now seeds `ExtraTextureMaps`) — green |
| #1658 | Prebaked (Skyrim) equip walks TPLT inheritance | PASS | Yes | `prebaked_equip_state_inherits_templated_inventory`, `prebaked_equip_state_uses_own_inventory_without_template` — green |

---

## Per-issue detail

### #1590: FO4 DLC/multi-master precombines resolve the wrong CSG + remapped path form-id
- **Status**: PASS
- **Closed**: 2026-06-19
- **Fix commit**: `d6bf8437` (merge `294cef4e`)
- **Fix site**: `byroredux/src/cell_loader/precombined.rs` (`resolve_precombine_owner`, `precombine_oc_nif_path`, `open_geometry_csg`)
- **Fix present**: Yes. CSG is now resolved against the cell's *owning* plugin (`owning_path`/`owning_subdir`) rather than the last-loaded `--esm`; the `_oc.nif` path is built via `precombine_oc_nif_path(cell.form_id, hash, owning_subdir)` which zeroes the remapped mod-index byte for DLC and routes through the owner subdir.
- **Guard test**: `oc_nif_path_dlc_uses_subdir_and_zeroes_mod_byte`, `resolve_precombine_owner_follows_form_id_mod_index` (+ base-game root case + end-to-end `dlc_precombine_path_and_csg_resolve_end_to_end`) — pass.
- **Notes**: The `BSPackedGeomObject.filename_hash` BSCRC32 cross-check is still documented as a future follow-up (not part of this fix's scope); the owning-plugin resolution is the shipped, validated mechanism. Not a regression.

### #1592: FO4 NIF shader-flag bits parsed but never consumed
- **Status**: PASS
- **Closed**: 2026-06-19
- **Fix commit**: `f7fbbed5` (merge `72b6223f`)
- **Fix site**: `crates/nif/src/import/material/walker.rs` (BSLightingShaderProperty arm, FO4-bsver-gated)
- **Fix present**: Yes. `MODEL_SPACE_NORMALS` (F4SF1 bit12, plus FO76+ CRC `MODELSPACENORMALS`) and `ALPHA_TEST` (F4SF2 bit25) are now OR'd into `MaterialInfo` as a lower-priority source than the BGSM merge, gated on `bsver >= FALLOUT4` so the Skyrim flag vocabulary is untouched.
- **Guard test**: 5 tests in `crates/nif/src/import/material/fo4_shader_flag_tests.rs` — pass.
- **Notes**: `GLOW_MAP` (F4SF2 bit6) is **intentionally not wired** — the glow texture is already captured from texture-set slot 2 and `emissive_source` is already `Lighting`, so the bit is redundant. Documented in the commit body; not a coverage gap.

### #1594: FO4 BSConnectPoint attach-point data lifted but never consumed
- **Status**: PASS
- **Closed**: 2026-06-19
- **Fix commit**: `c16600a5` (merge `72b6223f`)
- **Fix site**: `byroredux/src/cell_loader/references.rs` (`attach_points_component`, `child_attach_connections_component` via `extract_attach_points`/`extract_child_attach_connections`); stamped onto the root entity in `byroredux/src/cell_loader/spawn.rs` (`stamp_attach_graph`, reading `cached.attach_points` / `cached.child_attach_connections`); cache fields in `cell_loader/nif_import_registry.rs`.
- **Fix present**: Yes. The chain now runs parse → `ImportedScene` → interned `AttachPoints`/`ChildAttachConnections` in the import cache → stamped onto the spawned placement-root entity.
- **Guard test**: 3 tests in `cell_loader::attach_points_spawn_tests` — pass.

### #1606: Starfield LOD BSLightingShaderProperty under-reads +38 B
- **Status**: PASS
- **Closed**: 2026-06-19
- **Fix commit**: `497700e7` (merge `bcfe01f2`)
- **Fix site**: `crates/nif/src/blocks/shader.rs` (`read_starfield_tail`, field `starfield_tail`, gated `bsver >= STARFIELD`)
- **Fix present**: Yes. The Starfield (bsver ≥ 172) path now captures the trailing bytes between the parser stop and `block_size` as an opaque `starfield_tail` (length-agnostic), so consumed == `block_size`.
- **Guard test**: `parse_bs_lighting_starfield_tail_empty_without_size_or_drift` — pass.
- **Notes**: Tail captured as opaque bytes rather than decoded fields (the precise Starfield field semantics remain unreverse-engineered) — this is the correct fix for the under-read (stream position now reconciles); decoding the field's meaning is a separate, non-regressing follow-up.

### #1650: Oblivion 16-byte ACBS never parsed
- **Status**: PASS
- **Closed**: 2026-06-19
- **Fix commit**: `3d5d0d68` (merge `4d8c1d77`)
- **Fix site**: `crates/plugin/src/esm/records/actor.rs` (`parse_npc`, new `b"ACBS" if matches!(game, GameKind::Oblivion) && sub.data.len() >= 16` arm, ordered before the `>= 24` FNV/FO3/Skyrim+ arm)
- **Fix present**: Yes. The Oblivion arm reads `acbs_flags = u32 @0` and `level = i16 @10`; the FNV `>= 24` arm is unchanged.
- **Guard test**: `oblivion_16byte_acbs_parses_level_and_gender` (level > 1 + female gender) and `fnv_ignores_16byte_acbs` — pass.

### #1651: BGSM/BGEM blend factors forwarded with inverted enum
- **Status**: PASS
- **Closed**: 2026-06-19
- **Fix commit**: `ada75ee3` (merge `f4fd616b`)
- **Fix site**: `byroredux/src/asset_provider.rs` (`gl_to_gamebryo_blend`, applied at both the BGSM merge and the BGEM merge)
- **Fix present**: Yes. Both the BGSM (`merge`) and BGEM branches now route `src_blend`/`dst_blend` through `gl_to_gamebryo_blend` (swaps only 0↔1; 2..=10 coincide). The misleading "align 1:1" comment is replaced. The previous wrong-behavior test was rewritten to pin the *converted* additive `(One,One)` → Gamebryo `(0,0)`.
- **Guard test**: `gl_to_gamebryo_blend_swaps_only_zero_and_one`, `bgsm_merge_forwards_alpha_blend_mode` — pass.
- **Notes**: Canonical-boundary discipline preserved — the renderer still speaks only the Gamebryo `NiAlphaProperty` enum; conversion happens at the parser→Material merge.

### #1652: bhk motion_type → MotionType mapping wrong vs canonical Havok enum
- **Status**: PASS
- **Closed**: 2026-06-19
- **Fix commit**: `dc33ec7d` (merge `48b4f5c7`)
- **Fix site**: `crates/nif/src/import/collision.rs` (`havok_motion_type`)
- **Fix present**: Yes. The full enum is mapped: `1..=5 | 8 => Dynamic`, `6 => Keyframed`, `7 => Static`, `9 => CharacterKinematic`, `0 | _ => Static`. The old `4 => Keyframed` / `_ => Static` collapse is gone.
- **Guard test**: `havok_motion_type_maps_full_enum` (pins 4/5/8→Dynamic, 6→Keyframed, 7→Static, 9→CharacterKinematic, 0/out-of-range→Static) — pass.

### #1656: Unload-walk "all texture handle components" test omits ExtraTextureMaps
- **Status**: PASS
- **Closed**: 2026-06-19
- **Fix commit**: `2647e632`
- **Fix site**: `byroredux/src/cell_loader/unload_greyscale_lut_tests.rs` (test `unload_walk_collects_all_texture_handle_components`)
- **Fix present**: Yes. The fixture now constructs an `ExtraTextureMaps` entity with the 6 slots (glow/detail/gloss/parallax/env populated, env_mask=0 as placeholder) and asserts the non-zero handles are collected and the placeholder is not.
- **Guard test**: the test itself — pass. (Coverage-only fix; production `collect_victim_gpu_handles` was already correct.)

### #1658: Prebaked (Skyrim) equip state ignores TPLT inventory inheritance
- **Status**: PASS
- **Closed**: 2026-06-19
- **Fix commit**: `f5dba3d1`
- **Fix site**: `byroredux/src/npc_spawn.rs` (`build_npc_equip_state`)
- **Fix present**: Yes. The prebaked path now seeds inventory via the same game-agnostic `byroredux_plugin::equip::resolve_inherited_inventory(npc, actor_level, index)` helper as the kf-era path, so the TPLT chain is walked when `TEMPLATE_FLAG_USE_INVENTORY` is set.
- **Guard test**: `prebaked_equip_state_inherits_templated_inventory`, `prebaked_equip_state_uses_own_inventory_without_template` — pass.

---

## Step 4 — Unconditional fragile-area checks

| Contract | Status | Evidence |
|----------|--------|----------|
| Single material boundary (`translate_material` sole `ImportedMesh → Material`) | PASS | `byroredux/src/material_translate.rs::translate_material` is the only import-path constructor; other `Material { … }` sites are the `--cornell` reference scene and a unit-test helper, not import. |
| `Material::metalness` / `roughness` plain resolved `f32` | PASS | `crates/core/src/ecs/components/material.rs:217,223` — `pub metalness: f32` / `pub roughness: f32`, no `Option`, doc says "fully resolved, no Option". |
| Typed particle emitters dispatched | PASS | `NiPSysEmitter*` / `NiPSysGrowFadeModifier` dispatched as typed blocks in `crates/nif/src/blocks/mod.rs`; opaque `NiPSysBlock` retired (comment at `:1072`). |
| Collision shape coverage (`BhkMultiSphereShape`, `BhkConvexListShape`) | PASS | Both downcast and translated to `CollisionShape` in `crates/nif/src/import/collision.rs` (`:566`, `:684`). |
| Disney/Burley lobe split into `pbr.glsl`; reservoir array retired | PASS | `crates/renderer/shaders/include/pbr.glsl` + `lighting.glsl` exist; `resRadiance[]` appears only in removal-documenting comments; `shadowableLightRadiance()` present in `lighting.glsl`; no reservoir attachment in `gbuffer.rs`. |
| `#[repr(C)]` GPU struct size pins | PASS | `gpu_instance_is_112_bytes_std430_compatible`, `gpu_camera_is_336_bytes`, `gpu_material_size_is_300_bytes` + field-offset/GLSL-name pins — all green (`cargo test -p byroredux-renderer gpu_`). |

---

## Suite health (latent-regression sweep)

Full lib suites run green at HEAD:

- `byroredux-nif --lib`: 835 passed, 0 failed
- `byroredux-plugin --lib`: 500 passed, 0 failed, 13 ignored
- `byroredux-renderer --lib`: 335 passed, 0 failed
- `byroredux` (binary): all targeted regression tests green (binary crate has no lib target; bin tests run via `cargo test -p byroredux <name>`)

## Conclusion

No regressions. Every priority fix is present at its named symbol and guarded by
a green test; every Step-4 contract holds. Nothing to publish.
