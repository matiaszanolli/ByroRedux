# #1444 — LC-D9-01: NiPSysPartSpawnModifier undispatched, Oblivion catch-all cascade

_Snapshot as filed (2026-06-02) from AUDIT_LEGACY_COMPAT_2026-06-02.md. GitHub is authoritative for live state._

- **Severity**: LOW
- **Dimension**: D9 (Particle Emitter Translation Parity)
- **Location**: `crates/nif/src/blocks/mod.rs` (no `"NiPSysPartSpawnModifier"` case; catch-all `:1190-1225`)
- **Status**: NEW — instance of the #1332 class

## Description
`NiPSysPartSpawnModifier` (a real `NiPSysModifier` subclass per `nif.xml`) is not dispatched and falls to the catch-all. On FO3+/Skyrim+ the block_size recovery skips it gracefully; on **Oblivion (no block_sizes)** the catch-all hard-errors (`crates/nif/src/blocks/mod.rs:1217-1225`) and truncates the remaining NIF — the #1332 ceiling, triggered here by a specific untyped modifier.

## Impact
Bounded — depends on whether the modifier actually appears in Oblivion-era content (unverified; its `nif.xml` version range predates 20.0.0.5). The structural cascade is the real concern and is already tracked under #1332.

## Suggested Fix
Add a base-only `parse_modifier_only` arm (sibling of `NiPSysPositionModifier`) so it skips cleanly on every era.

## Related
#1332.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (KF-sequence path vs embedded path; other channel converters)
- [ ] **CANONICAL-BOUNDARY**: per-game logic stays at the NIFAL parser→Material/clip boundary, never pushed to shaders/renderer or re-derived at render time. See /audit-nifal.
- [ ] **TESTS**: Regression test added for this specific fix

_Filed from [docs/audits/AUDIT_LEGACY_COMPAT_2026-06-02.md](../blob/main/docs/audits/AUDIT_LEGACY_COMPAT_2026-06-02.md)_
