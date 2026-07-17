# OBL-D7-01: legacy_particle.rs module doc overclaims Oblivion dependency the real corpus contradicts

- **Severity**: LOW
- **Labels**: low, nif-parser, documentation
- **Location**: `crates/nif/src/blocks/legacy_particle.rs:1-17`

## Description
The module doc asserts Oblivion "still serializes" the pre-10.1 legacy particle stack (`NiParticleSystemController`, `NiAutoNormalParticles`, `NiRotatingParticles`, etc.), directly contradicted by `crates/nif/src/import/walk/mod.rs:502-505`'s comment ("the target games all author the modern NiParticleSystem stack") and by real corpus data: the checked-in per-block-type baselines for all 7 supported games, including Oblivion's own 8032-NIF sweep, show zero occurrences of any legacy-stack block type. Oblivion's baseline shows `NiParticleSystem 547 0` — 547 correctly-typed modern particle systems, the type that *is* routed to the renderer.

## Evidence
The module doc at `legacy_particle.rs:9-11` states "Bethesda kept these types alive well past nif.xml's `until="10.0.1.0"` — Oblivion is v20.0.0.5 and still serializes them." `crates/nif/tests/data/per_block_baselines/oblivion.tsv` (and the 6 sibling per-game TSVs) contain no `AutoNormal`/`Rotating`/`ParticleSystemController`/`BSPArray` rows — grepping for those block types plus `NiParticleSystem` yields exactly one matching row: `NiParticleSystem 547 0`. `git show 23ab46f2` (#1327) confirms the legacy-emitter surfacing arm was dead code (never reachable) even before its removal.

## Impact
Low — doc-rot only. The parser itself may be legitimate nif.xml-completeness / defensive coverage (mod content, non-`Meshes.bsa` archives, or other NetImmerse-era titles), but as written the doc would send a future auditor chasing a "dropped Oblivion particle FX" finding that the real data refutes — exactly the stale-premise pattern flagged by `feedback_audit_findings.md`.

## Related
#1327 (dead-arm removal this doc should have been updated alongside)

## Suggested Fix
Soften the module doc to state the format support is nif.xml-driven/defensive rather than asserting vanilla Oblivion content requires it, citing the per-block baseline evidence; or if a non-`Meshes.bsa` Oblivion source is later found to ship these blocks, cite that source instead.

## Completeness Checks
- [ ] **SIBLING**: Check other block-parser module docs for the same stale "still ships on game X" framing now that per-block baselines exist for all 7 games
