# #3534 — LC-2026-08-27-D5-01: the /audit-legacy-compat skill's own Dimension 5 text asserts two gaps that #3321 and the VWD consumer wiring have since closed

Labels: low, documentation, doc-rot, legacy-compat, terrain-exterior
Source: docs/audits/AUDIT_LEGACY_COMPAT_2026-08-27.md (base 969d81c8)
Filed: 2026-08-27 via /audit-publish

---

**From:** `docs/audits/AUDIT_LEGACY_COMPAT_2026-08-27.md` (LC-2026-08-27-D5-01) · base `969d81c8`

- **Severity**: LOW
- **Dimension**: 5 — EXAL, audit-infrastructure currency
- **Location**: `.claude/commands/audit-legacy-compat/SKILL.md`, Dimension 5's "LOD — status" bullet (the `distantlod\*.lod` clause and the "zero consumers" clause)

## Description

Two of that bullet's load-bearing claims are false at HEAD.

1. *"FO3/FNV ship **zero** `distantlod\*.lod` files in any vanilla archive (FO3-D4-01 / #2086) — 'FO3/FNV distant-object LOD is missing' is a real gap but not a `placement_lod.rs` gap."* The first half still holds and `placement_lod_supported` is still Oblivion-only (`byroredux/src/cell_loader/placement_lod.rs:313`, pinned at `:754-760`). The second half is now wrong: `e23a9908` (#3321, 2026-08-27) established that FO3/FNV ship a **third** scheme — `meshes\landscape\lod\<world>\blocks\<world>.level<L>.x<qx>.y<qy>.nif`, structurally the `.bto` shape in a different container — and consumed it as an `ObjectLodScheme` arm inside the existing `byroredux/src/cell_loader/object_lod.rs` (module doc rewritten at `:18-37` to say so explicitly; `object_lod_scheme` at `:458-461` now maps `GameKind::Fallout3NV => ObjectLodScheme::FalloutLegacyBlocks`). The commit records a live verification of 280 quads loaded on `WastelandNV (0,0)` where the pre-fix engine loaded 0. FO3/FNV distant-object LOD is no longer a gap.
2. *"The **VWD / 'Has Distant LOD' record-header flag** is now parsed and exposed (`RecordHeader::is_visible_when_distant()`, #1731) but **has zero consumers**."* It has consumers: the flag is captured onto placements (`crates/plugin/src/esm/cell/support.rs:361,599,663,728`), stamped as an ECS row by `stamp_visible_when_distant` (`byroredux/src/cell_loader/references/synth_child.rs:682,744-750`), and read by the LOD reconcile loop (`resident_vwd_refr_cells`, `byroredux/src/streaming_helpers.rs:185,315`, cited by **#3142 OPEN**). What remains open is *full-model culling* from it, which is tracked as **#3307 OPEN** ("EX-10/11 item 8: active VWD full-model culling") — a narrower and differently-owned gap than "zero consumers".

## Evidence

`e23a9908`'s diff and commit body; the greps cited above (both `ObjectLodScheme::FalloutLegacyBlocks` and `resident_vwd_refr_cells` re-confirmed at HEAD); the corrected `docs/engine/exal.md` §5 (the commit rewrote it in the same change, so the *doc* is current and only the *skill* lagged).

## Impact

Documentation-only, but self-inflicted on the audit pipeline: Dimension 5 tells the auditor these are real coverage gaps ("Findings here are real coverage gaps, not premise errors"), so the next sweep is being actively steered toward re-filing two closed items. This is the third consecutive legacy-compat sweep whose yield includes stale audit reference material.

## Related

LC-D6-2026-08-24-01 and LC-D6-03 (2026-08-20) — same class, different documents. #3321 (closed), #3307 (open), #3142 (open). Same class filed by siblings this run: #3422, #3511.

## Suggested Fix

Replace the FO3/FNV clause with the post-#3321 state (three schemes: Oblivion placement lists → `placement_lod.rs`; FO3/FNV `blocks\` quads and Skyrim/FO4 `.bto` → `object_lod.rs`), and replace "zero consumers" with "consumed for LOD residency; full-model culling is #3307". `docs/engine/exal.md` §5 is already correct and can be quoted directly.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other audit SKILL files and `docs/feature-matrix.md` rows that describe FO3/FNV distant-object LOD or the VWD flag)
