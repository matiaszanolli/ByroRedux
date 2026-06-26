# TD2-001: NiDynamicEffect base read duplicated across light.rs/texture.rs (divergent-fix history)

_Filed 2026-06-26 as #1750 from docs/audits/AUDIT_TECH_DEBT_2026-06-26.md (immutable snapshot; query `gh issue view 1750` for live state)._

**Severity**: MEDIUM (duplicated logic with *proven* divergent-fix history) · **Dimension**: 2 — Logic Duplication
**Location**: `crates/nif/src/blocks/light.rs:72-85` ↔ `crates/nif/src/blocks/texture.rs:899-911`
**Status**: NEW · **Audit**: TD2-001

## Description
The `NiDynamicEffect` base read — `pre_fo4` BSVER gate + `switch_state` u8 (V10_1_0_106) + `affected_nodes` u32-count array (V10_1_0_0) — is byte-identical across the only two `NiDynamicEffect` subclasses (`NiLightBase`, `NiTextureEffect`). texture.rs:888-898 even says "same version gates as NiLight. See light.rs."

## Evidence — divergent-fix history
- light.rs copy fixed under **#721**.
- texture.rs copy was MISSED in that sweep and got the identical fix ~500 commits later under **#1240**.
- Between #721 and #1240, FO4 `NiTextureEffect` over-read 5+ bytes per block (every mesh-embedded texture-effect → NiUnknown block_size recovery).

## Impact
The surface is closed today only because these are the only two `NiDynamicEffect` subclasses; the next change to this base must again be applied by hand in two places — exactly the failure mode that already bit the codebase once.

## Suggested Fix
Add `NiDynamicEffectData::parse(stream) -> io::Result<(bool, Vec<u32>)>` to `crates/nif/src/blocks/base.rs` (alongside `NiObjectNETData` / `NiAVObjectData`); both parsers call it immediately after `NiAVObjectData::parse`.

## Completeness Checks
- [ ] **SIBLING**: all `NiDynamicEffect` subclasses route through the new helper (light + texture; confirm no third)
- [ ] **TESTS**: a regression test pins the FO4 `NiTextureEffect` field offset (the #1240 case)
