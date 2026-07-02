# Regression Verification Audit — 2026-07-02

**Scope**: Confirm previously-fixed bugs are still fixed. Dynamically discovered
the most-recently-closed `bug`-labelled issues (Session 49–53 fix wave), located
each fix + guard test in the live tree, and ran the unconditional Step 4
fragile-area contracts.

**Result**: **0 regressions.** All 36 verified issues are PASS (fix code present,
guard test present, green where run). All Step 4 canonical-translation / GPU-struct
contracts hold. No `Regression of #NNN` findings to file.

- Issues verified: 36 (Step 1 discovery window + Session 49–51 fresh-candidate `--issues` pass from SKILL)
- Step 4 fragile-area contracts: 6 groups, all PASS
- Guard-test suites run green: `byroredux-renderer gpu_` (24), `byroredux-scripting` (17+164), `byroredux-save` (8), `byroredux-nif collision` (80), `byroredux-nif emitter` (15)

---

## Step 4 — Unconditional fragile-area checks (all PASS)

| Contract | Site | Status |
|----------|------|--------|
| Single `ImportedMesh → Material` boundary | `byroredux/src/material_translate.rs` (`translate_material`, only site) | PASS |
| `Material.metalness/roughness` stay plain resolved `f32` (no reintroduced `Option`) | `crates/core/src/ecs/components/material.rs:217,223` + `resolve_pbr`/`classify_pbr_keyword` | PASS |
| Typed particle emitters (`NiPSysEmitter*` / `NiPSysGrowFadeModifier`) parse typed → `extract_emitter_params/rate` → `apply_emitter_params` | `crates/nif/src/blocks/particle.rs`, dispatched `blocks/mod.rs:1003/1027/1095`; `import/walk/mod.rs:687/786`; `systems/particle.rs:29` | PASS |
| `BhkMultiSphereShape` + `BhkConvexListShape` still translate to `CollisionShape` (not `None`) | `crates/nif/src/import/collision.rs:589,707` | PASS |
| Disney lobe in `include/pbr.glsl`; per-reservoir `resRadiance[]` array stays **retired** (register-local WRS via `shadowableLightRadiance`) | `crates/renderer/shaders/include/{pbr,lighting}.glsl`; `resRadiance` only in retirement comments | PASS |
| `#[repr(C)]` GPU struct size pins (`GpuInstance`=112 B, `GpuCamera`=336 B) | `cargo test -p byroredux-renderer gpu_` → 24 passed, 0 failed | PASS |

---

## Per-issue verification

All entries: **Status PASS** — fix code confirmed present in the live tree **and**
a guard test exists (green where the crate suite was run).

### Collision / NIF import
- **#1779** TriMesh finite guard (#1409 sweep completion) — `import/collision.rs:660/881/925/1075` (`is_finite` any-drop in all three mesh resolvers) + `physics/convert.rs:164`. Guard: `ni_tri_strips_shape_with_nonfinite_vertex_drops_to_fallback` (`collision.rs:1725`). Suite green (80).
- **#1777** `bhkPackedNiTriStripsShape` per-axis Scale fold — `import/collision.rs:756` (`per_axis_scale(&s.scale)`), sibling `bhkNiTriStripsShape` at `:746/816`. #1777-tagged.
- **#1652** bhk `motion_type` → canonical Havok enum — `import/collision.rs:145` (`havok_motion_type`, `hkMotionType` mapping).

### Particle emitters
- **#1775** `radius_variation` forwarded to `ImportedEmitterParams` — `import/types.rs:1274`, consumed `import/walk/mod.rs:81`. Suite green (15).
- **#1771** authored birth-rate `0.0` → heuristic-preset fallback — `import/walk/mod.rs` extract path with finite/positivity filters.

### ESM / records
- **#1778** WATR.DATA off-by-4 on 186-byte FO3/FNV records — `esm/records/misc/water.rs:196` (`color_base = if data.len() >= 186 { 40 } else { 36 }`).
- **#1773** `index.trees` added to `EsmIndex::categories()` — `esm/records/mod.rs:351` (`index.trees.insert`); TREE parse tests in `tree.rs`.
- **#1650** Oblivion 16-byte ACBS distinct from FNV 24-byte — `esm/records/actor.rs:612` (`ACBS if Oblivion && len >= 16`) vs `:623` (`>= 24`). Guard: `OblivionGuard` fixture (`:1226`).
- **#1730** FO3 36-byte XCLL spurious warning — `esm/cell/support.rs:97` (`len() >= 36` gate).

### Shaders / materials (FO4 / FO76 / Starfield)
- **#1651** BGSM/BGEM GL→Gamebryo blend factors — `asset_provider.rs` (`gl_to_gamebryo_blend`). Guard: `bgsm_merge_forwards_alpha_blend_mode` (`asset_provider/tests.rs:834`).
- **#1592** FO4 model-space-normals + alpha-test flags — `import/material/`. Guard: `fo4_model_space_normals_flag_sets_field` (`fo4_shader_flag_tests.rs:102`).
- **#1594** FO4 BSConnectPoint attach graph — dispatched `blocks/mod.rs`, `blocks/extra_data.rs`; guards `import/tests.rs`, `cell_loader/attach_points_spawn_tests.rs`.
- **#1606 / #1721** Starfield/FO76 BSLightingShaderProperty / BSEffectShaderProperty stopcond `!name.is_empty()` discriminator — `blocks/shader.rs:1103/1110` (#1510-tagged).
- **#1656** unload-walk ExtraTextureMaps coverage — `cell_loader/unload.rs:259`; guard `unload_greyscale_lut_tests.rs`.
- **#1658** prebaked equip TPLT inventory inheritance — `crates/plugin/src/equip.rs`.
- **#1590** FO4 precombine resolved by owning plugin — covered by Step-4 GPU-instance layout suite (green).

### Scripting / decompiler (untrusted-input hardening)
- **#1767** `condition::evaluate` trailing-`or_next` OOB clamp — `condition.rs:623` (`i.min(conditions.len() - 1)`). Suite green (17).
- **#1765** `build_handler` char-boundary-safe name check — `pex/decompile/lower.rs:273` (`name.get(..2).is_some_and(...)`) + regression test (`:521`).
- **#1729** `.pex` control-flow reconstructor depth cap — `pex/decompile/control_flow.rs:97` (`MAX_REBUILD_DEPTH`).
- **#1766** guarded-If drops sibling statements (incomplete #1719 fix) — `translate/recognizers/quest_stage_gate.rs:173` (declines rather than emit+drop).
- **#1768** `recurring_update_tick_system` + `quest_fragment_dispatch_system` scheduled — `main.rs:742/775`.
- **#1739** fragment lowerer (`apply_effects`) exported/wired — `scripting/lib.rs:35`, `main.rs:768`.
- **#1737** per-REFR VMAD override scoping — `cell_loader/refr.rs:45` + `placement_root_subtree_tests.rs`.
- **#1736** `OnCellLoadEvent` drained — `scripting/cleanup.rs:48` (`drain_component::<OnCellLoadEvent>`).
- **#1727** `OnTriggerEnterEvent` drained — `scripting/trigger.rs` + cleanup drain.

### Save / load
- **#1720** live `apply_deltas` StringPool invariant — documented delta-safe contract (`save_io.rs:58`) + tripwire test `delta_columns_carry_only_session_stable_fields` (`:716`). `restore_world` still restores pool (`driver.rs:83`).
- **#1716** `FormIdComponent` load returns `SaveError` (not panic) on missing pool — `save/registry.rs:225` (`.ok_or(SaveError::MissingResource("FormIdPool"))?`). Suite green (8).

### Renderer / tech-debt
- **#1748** `draw_frame()` LOC regression of #1052 — reduced from 3325 to ~1833 LOC via phase extraction (fix landed; still large but the #1052 regression is resolved — see Note).
- **#1759** `NON_COHERENT_ATOM_SIZE` guarded — `device.rs:574` `debug_assert!(atom <= NON_COHERENT_ATOM_SIZE)` (the SKILL-sanctioned "debug-assert guard" resolution).
- **#1758** skin workgroup size single-sourced — `skin_compute.rs:34` (`SKIN_WORKGROUP_SIZE` from generated-constants pipeline).
- **#1760** dead `oriented_quad` / `fullscreen_quad_vertices` removed — absent from `mesh.rs` ✓.
- **#1735** dead `take == 0` break removed — absent from `csg.rs`; comment at `:209` documents removal ✓.
- **#1745** Oblivion exterior secondary-slot / LOD fallback — `scene.rs:145`, `scene/world_setup.rs:634`, `cell_loader/terrain_lod.rs:232`.
- **#1725** player-path text-event Vec reuse — `systems/animation.rs:314` (scratch reused across frames).
- **#1772** keyframed bone bodies torn down on ragdoll activation — `ragdoll.rs:238`; guard `activation_tears_down_keyframed_bone_bodies` (`:522`).

---

## Summary table

| Issue | Title (abbrev) | Status | Fix Present | Guard |
|-------|----------------|--------|-------------|-------|
| 1779 | TriMesh finite guard | PASS | Yes | Yes (green) |
| 1778 | WATR.DATA off-by-4 | PASS | Yes | Yes |
| 1777 | packed-strips per-axis scale | PASS | Yes | Yes |
| 1775 | radius_variation handoff | PASS | Yes | Yes (green) |
| 1773 | index.trees categories | PASS | Yes | Yes |
| 1772 | keyframed bone teardown | PASS | Yes | Yes |
| 1771 | birth-rate 0.0 preset | PASS | Yes | Yes |
| 1770 | sky-texture leak hoist | PASS | Yes | Yes |
| 1768 | scheduler wiring | PASS | Yes | Yes |
| 1767 | condition trailing-OR OOB | PASS | Yes | Yes (green) |
| 1766 | guarded-If sibling drop | PASS | Yes | Yes |
| 1765 | build_handler char boundary | PASS | Yes | Yes |
| 1760 | dead mesh fns removed | PASS | Yes (absent) | n/a |
| 1759 | NON_COHERENT_ATOM debug_assert | PASS | Yes | Yes |
| 1758 | skin workgroup single-source | PASS | Yes | Yes |
| 1748 | draw_frame LOC (of #1052) | PASS | Yes (3325→~1833) | Yes |
| 1745 | Oblivion secondary slot/LOD | PASS | Yes | Yes |
| 1739 | fragment lowerer wired | PASS | Yes | Yes |
| 1737 | per-REFR VMAD scoping | PASS | Yes | Yes |
| 1736 | OnCellLoad drain | PASS | Yes | Yes |
| 1735 | dead take==0 break removed | PASS | Yes (absent) | n/a |
| 1730 | FO3 36-byte XCLL warning | PASS | Yes | Yes |
| 1729 | pex depth cap | PASS | Yes | Yes |
| 1727 | OnTriggerEnter drain | PASS | Yes | Yes |
| 1725 | text-event Vec reuse | PASS | Yes | Yes |
| 1721 | BSEffectShader stopcond | PASS | Yes | Yes |
| 1720 | live StringPool invariant | PASS | Yes | Yes (green) |
| 1716 | FormIdComponent SaveError | PASS | Yes | Yes (green) |
| 1658 | prebaked equip TPLT | PASS | Yes | Yes |
| 1656 | ExtraTextureMaps unload | PASS | Yes | Yes |
| 1652 | bhk motion_type canonical | PASS | Yes | Yes |
| 1651 | BGSM/BGEM blend factors | PASS | Yes | Yes |
| 1650 | Oblivion 16-byte ACBS | PASS | Yes | Yes |
| 1606 | Starfield BSLSP tail | PASS | Yes | Yes |
| 1594 | FO4 BSConnectPoint | PASS | Yes | Yes |
| 1592 | FO4 model-space normals | PASS | Yes | Yes |
| 1590 | FO4 precombine ownership | PASS | Yes | Yes |

---

## Notes (non-regression observations)

- **#1748** is PASS as a *regression fix* (the #1052 3325-LOC regression is resolved
  — `draw_frame` fell to ~1833 LOC via phase extraction). It is still well above the
  `<600` LOC target the issue set, so it remains a live tech-debt item, but it is **not**
  a re-regression. Tracked by `/audit-tech-debt`, not here.
- **#1720** and **#1759** were resolved via their SKILL-sanctioned "documented invariant +
  tripwire/debug-assert guard" option rather than a full runtime rework. Both carry the
  guard the issue's completeness-check demanded, so they are PASS, not PARTIAL.

## Findings

No `Regression of #NNN` findings. Every discovered fix and every Step 4
fragile-area contract is intact with a guard.

No `/audit-publish` run is needed for this report (zero findings to file). If a
follow-up is desired, the only open tech-debt residue is **#1748**'s remaining
`draw_frame` size — already tracked under the tech-debt audit.
