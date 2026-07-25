# NIF-META-01: audit-nif skill's Dimension 2 checklist cites 7 NifVariant helper names that no longer exist (all deleted as dead code)

- **GitHub Issue**: #2169
- **Severity**: LOW
- **Dimension**: Version Gating (audit-tooling meta-finding, not a codebase defect)
- **Location**: `.claude/commands/audit-nif/SKILL.md` (Dimension 2 checklist, the "canonical helpers" sentence); actual code at `crates/nif/src/version.rs`
- **Source Report**: `docs/audits/AUDIT_NIF_2026-07-25.md`
- **Labels applied**: `documentation`, `low`, `tech-debt`

## Description

The skill's Dimension 2 checklist states the canonical `NifVariant`-keyed feature-flag helper surface "currently include[s] `has_properties_list`, `has_effects_list`, `has_culling_mode`, `has_shader_alpha_refs`, `has_shader_property_fo3_fields`, `uses_bs_tri_shape`, `has_material_crc`" (plus the still-live `NifVersion`-side collision/v10.x helpers).

All seven of those `NifVariant` names have been deleted — `#1840` removed six of them, `#1897` removed the seventh survivor (`has_shader_property_fo3_fields`), per an explicit removal-log comment at `crates/nif/src/version.rs:560-563`.

```
grep -rn "has_properties_list\|has_effects_list\|has_culling_mode\|has_shader_alpha_refs\|has_shader_property_fo3_fields\|uses_bs_tri_shape\|has_material_crc" crates/nif/src/version.rs
```
only matches the removal-log comment, not a live definition. Today `impl NifVariant` has exactly two methods: `detect` and `bsver`. The 8 collision/v10.x-era helpers the checklist lists as living on the same surface (`has_mopp_offset`, `has_object_group_id`, etc.) DO still exist — correctly — but they live on `NifVersion`, not `NifVariant`.

`docs/engine/nif-parser.md`'s own "Version handling" section already describes the current (post-revert) doctrine accurately without naming any of the seven dead helpers. This is the same class of doc-rot the prior `AUDIT_NIF_2026-07-16.md` flagged against `docs/engine/nif-parser.md` — except this time the stale reference lives in the audit skill's own checklist text, not the architecture doc.

## Impact

None to the running engine. Risk is entirely to future audits: someone could grep for one of the seven names, find nothing, and misdiagnose "a version-gating helper regressed" when in fact its deletion was the intentional, already-tracked `#1840`/`#1897` cleanup. Both the Dimension 2 sub-pass and the orchestrating session in the 2026-07-25 NIF audit independently hit this exact trap and had to disprove it before concluding "no finding."

## Suggested Fix

Update `.claude/commands/audit-nif/SKILL.md`'s Dimension 2 checklist to drop the seven dead `NifVariant` names and either (a) point at `docs/engine/nif-parser.md`'s "Version handling" section for the current doctrine, or (b) list the 8 `NifVersion`-side helpers that are actually live today (`has_object_group_id`, `has_mopp_offset`, `has_havok_strips_scale`, `has_skin_data_partition_ref`, `has_keyframe_controller_data`, `has_interp_controller_manager_controlled`, `has_quat_transform_trs_valid`, `uses_old_rigid_body_layout`).
