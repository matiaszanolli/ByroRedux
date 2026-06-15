# Issue #1543: OBL-D1-NEW-01: NiInterpController descendants bypass parse_interp_controller_base (Manager Controlled bool missing on v10.1.0.104-108)

_Snapshot as filed via /audit-publish from docs/audits/AUDIT_OBLIVION_2026-06-14.md. GitHub is authoritative for current state._

**Severity**: HIGH · **Dimension**: 1 (NIF Version Handling) · **Status**: NEW (sibling of resolved #1506 — same root field, different parser arms the #1506 fix did not cover)

**Location**:
- `crates/nif/src/blocks/controller/shader.rs:181` (`NiMaterialColorController`)
- `crates/nif/src/blocks/controller/shader.rs:57` (`NiLightColorController`)
- `crates/nif/src/blocks/controller/shader.rs:214` (`NiTextureTransformController`)
- `crates/nif/src/blocks/particle.rs:932` (`parse_emitter_ctlr`)
- `crates/nif/src/blocks/particle.rs:922` (`parse_modifier_ctlr`)

## Description
The #1506 fix added the `NiInterpController.Manager Controlled` bool (nif.xml `since=10.1.0.104 until=10.1.0.108`) only to `NiSingleInterpController::parse` (`controller/mod.rs:84` via `parse_interp_controller_base`). Several `NiInterpController` descendants are decoded by hand-rolled functions that still call the plain `NiTimeControllerBase::parse` and therefore skip the bool in that version band:
- `NiMaterialColorController` / `NiLightColorController` — nif.xml ancestry `NiPoint3InterpController → NiSingleInterpController → NiInterpController`.
- `NiTextureTransformController` — `NiFloatInterpController → NiSingleInterpController → NiInterpController`.
- `parse_emitter_ctlr` (`NiPSysEmitterCtlr`) / `parse_modifier_ctlr` (`NiPSysModifier*Ctlr`) — `NiPSysModifierCtlr → NiSingleInterpController → NiInterpController`.

Missing the 1-byte bool under-reads the block by 1. Oblivion v10.1.0.x has **no per-block size table**, so the drift cascades and truncates the whole downstream subtree.

## Evidence
`nif_stats` over `Oblivion - Meshes.bsa` (2026-06-15) = 8024/8032 clean, 8 truncated. Six are pre-Gamebryo `marker_*.nif`; the other two are this bug:
- `meshes\oblivion\architecture\citadel\interior\switch\scampswitch01.nif` (v10.1.0.106) drops **42 blocks**; `trace_block` shows drift first appearing at the two `NiMaterialColorController` blocks (#9/#10), then garbage `unknown KeyType: 16744447` at `NiTransformData`.
- `meshes\dungeons\ayleidruins\interior\arwelkydclusterfx01.nif` (v10.1.0.106) drops **15 blocks**; `trace_block` shows `NiPSysEmitterCtlr` (block 17) consuming a bogus **4646 bytes**, then a ~1 GB allocation attempt at the next `NiTexturingProperty`.

Inherit chains confirmed in `/mnt/data/src/reference/nifxml/nif.xml`: `NiInterpController.Manager Controlled` `since=10.1.0.104 until=10.1.0.108`; `NiPoint3InterpController`/`NiFloatInterpController` both inherit `NiSingleInterpController`. `has_interp_controller_manager_controlled()` (`version.rs:235`) is true for V10_1_0_104..V10_1_0_108 — the band these files sit in.

## Impact
Closes the last two non-marker Oblivion-Meshes truncations (→ 8030/8032, only the marker family remaining). Any old-Gamebryo (10.1.0.104–108) content with animated material/light color, animated UV transforms, or particle emitters silently loses the tail of its scene graph. Oblivion-only regression class.

## Related
#1506 (resolved — the `NiSingleInterpController::parse` half), OBL-D1-NEW-02 (compounds on the same files).

## Suggested Fix
Replace the bare `NiTimeControllerBase::parse` with the shared `parse_interp_controller_base` (or call `NiSingleInterpController::parse` and append the per-type tail) at all five sites. Promote `parse_interp_controller_base` to `pub(crate)` so `particle.rs` can use it. Add v10.1.0.106 regression fixtures for `NiMaterialColorController` + `NiPSysEmitterCtlr` mirroring the #1506 tests.

## Completeness Checks
- [ ] **SIBLING**: All five controller arms fixed (not just the one that surfaced in the trace); other `NiSingleInterpController` descendants audited for the same bypass
- [ ] **CANONICAL-BOUNDARY**: Fix stays at the NIF parser; no per-game logic pushed into the import/material/render path
- [ ] **TESTS**: A regression test pins this specific fix (v10.1.0.106 `NiMaterialColorController` + `NiPSysEmitterCtlr` fixtures)
