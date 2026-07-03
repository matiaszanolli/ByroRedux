# #1818: SCR-D6-NEW-01: feature-matrix.md understates the CTDA condition catalog (says 7, ships 13)

**Severity**: LOW
**Location**: `docs/feature-matrix.md:137`

The matrix row reads "CTDA condition evaluation with OR-precedence (M47.1) | ✓ 7 functions".
`condition.rs` ships 13 catalogued functions (GetDistance, GetActorValue, GetStage,
GetStageDone, GetIsClass, GetIsRace, GetIsID, GetFactionRank, GetLevel, HasPerk,
GetXPForNextLevel, GetReputation, GetReputationThreshold).

Fix: update the count to 13.

## Completeness Checks
- [x] **TESTS**: N/A — documentation-only fix

---

# #1819: SPT-NEW-05: Foliage texture-path substring collisions in the PBR keyword classifier mis-tag vanilla trees as wood/glass

**Severity**: HIGH
**Location**: `crates/core/src/ecs/components/material.rs:449-489` (`classify_pbr_keyword`),
`crates/spt/src/import/mod.rs:328-334` (`placeholder_billboard_mesh`)

The SpeedTree placeholder billboard is the only production content type that reaches
`resolve_pbr`'s keyword-classifier backstop arm — every NIF mesh extractor classifies
at import time and sets `metalness_override: Some(...)`. `placeholder_billboard_mesh`
sets both overrides to `None`, so `classify_pbr_keyword` runs substring matching
against the leaf texture path with no word-boundary check:
- `ShrubBoxwoodLeaves*.dds` contains "wood" → WOOD (roughness 0.7) instead of foliage
  default (0.85).
- `ShrubGenericElderberryLeaves*.dds` contains "ic"+"e" across the word seam
  ("generICE lderberry") → GLASS (roughness 0.1), crossing the RT-reflection gate
  (`< 0.6`) — visible "glass leaf" artifact.

## Suggested Fix
(a) Have `placeholder_billboard_mesh` set explicit
`metalness_override: Some(0.0)` / `roughness_override: Some(0.85)` so the
SpeedTree importer classifies-at-import like every NIF path. Lower-risk,
narrow, parity-preserving — no shared-classifier change.

## Completeness Checks
- [ ] **SIBLING**: Full FO3/FNV `.spt`-backed foliage texture corpus scanned for other
      keyword-substring collisions
- [ ] **CANONICAL-BOUNDARY**: Fix stays at the parser→Material boundary
- [ ] **TESTS**: Regression test pins Boxwood/Elderberry → foliage-default roughness
