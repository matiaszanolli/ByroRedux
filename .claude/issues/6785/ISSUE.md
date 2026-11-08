# ESM-D2-05: Two comments contradict the decoders beside them (PERK Starfield `DATA`, REGN `RDMO`)

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/6785
Labels: low, esm-plugin, documentation, doc-rot
Source: docs/audits/AUDIT_ESM_2026-11-08.md

_Filed from `docs/audits/AUDIT_ESM_2026-11-08.md` (audit HEAD 3500d2737)._

- **Severity**: LOW (doc-rot)
- **Dimension**: Sub-Record Byte Accounting (cross-ref ESM→ECS handoff / doc)
- **Location**:
  - `crates/plugin/src/esm/records/misc/magic.rs:497-499` says "Starfield's DATA differs and is still read in this order (PERK-DEC-009)"; the arm directly below (`:500-511`) is the Starfield-specific decode that #6652 added.
  - `crates/plugin/src/esm/records/misc/world.rs:748-750` says Skyrim `RDMO` is "Not decoded here (#3811) ... two new record types plus a track-selection policy", and `:756-763` says the field "structurally misses on every game". `03f9d2158` decoded `MUSC` and `MUST` and resolves `RDMO` to a track (all 28 vanilla `RDMO` resolve; `byroredux/src/asset_provider/audio.rs` uses `first_playable_track`).
- **Status**: NEW
- **Impact**: misleads the next reader of the REGN payload and the PERK `DATA` arm. No behaviour change.
- **Suggested fix**: rewrite both comments to state the current behaviour.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related decoders / other games' arms
- [ ] **LOCK_ORDER**: n/a unless a RwLock scope changes
- [ ] **TESTS**: A regression test pins this specific fix (real-data pin where an install exists)
