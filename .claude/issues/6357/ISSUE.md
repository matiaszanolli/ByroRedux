# TD1-2026-10-25-03: `byroredux/src/npc_spawn.rs` 1911 prod LOC (+149 since 10-21; its two extracted siblings grew +230 and +197) — the 10-15 split did not stop growth

Labels: low, gameplay, ai, tech-debt, bug
Issue: https://github.com/matiaszanolli/ByroRedux/issues/6357
(Immutable snapshot of the issue as filed; GitHub is authoritative for state.)

- **Severity**: LOW · **Dimension**: 1 · **Effort**: medium
- **Age**: 6 commits on the hub and 22 on `npc_spawn/` since 4a3e89d56 (`9d1a262ed` #6090, `3dc16208e`, `a6ab3625e` #5753, `1d319a942`, `1659d43c0` #6231, `cafd3ccab` #6227). Hub-hazard: the hub's own log understates churn; family sizes: `npc_spawn.rs` 1911, `npc_spawn/ai_package.rs` 1164 -> 1394 (+230), `npc_spawn/loot_appearance.rs` 1133 -> 1330 (+197), plus `resumable/`, `seam_blend/`.
- **Location**: `byroredux/src/npc_spawn.rs` (prod 1-1913, `mod tests;` external)
- **Structure read** (by line): stamps of ECS components from a `ResolvedNpc` L54-400 (faction ranks, spells, AVs, creature attack, ruleset/VATS/melee builders) · fallback collider/ragdoll install L401-560 · skeleton/body/creature **path resolution** L561-880 (`humanoid_skeleton_path`, `npc_skeleton_path`, `humanoid_body_paths`, `humanoid_body_path_icon_index`) · **animation clip loading + idle desync** L879-1320 (`load_idle_clip`, `load_kf_clip_by_path`, walk-clip tables, `idle_desync`, `SpawnIdle`) · **equipment** components + `build_npc_equip_state` L1318-1853 (the function itself is ~345 lines) · FaceGen prebaked paths + `tag_descendants_as_actor` L1854-1913.
- **Split axis**: `npc_spawn/{stamps.rs (L54-400), fallback_physics.rs (L401-560), skeleton_paths.rs (L561-880 + facegen paths), anim_clips.rs (L879-1320), equip.rs (L1318-1853)}`; the hub keeps `mod` decls + `pub(crate)` re-exports so `crate::npc_spawn::X` callers (cell_loader, scene, tests) are unchanged. Move `build_npc_equip_state` verbatim first (pure move), extract its phases only afterwards (it is the >300-line fn on this file's list).
- **Hazards**: `npc_spawn/tests.rs:3288` `include_str!("../npc_spawn.rs")` and :3375, :3416 census entries; `material_translate.rs:2817` names `"npc_spawn.rs"` in a scan list. Both must be re-pointed in the same commit.

---
Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-25.md` (HEAD c7463c04d), finding `TD1-2026-10-25-03`. Fix with: `/fix-issue <this issue>`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / crates
- [ ] **TESTS**: A regression test or source-scan guard pins this specific fix (or the finding is doc-only and `_audit-validate.sh` / `cargo test` stays green)
