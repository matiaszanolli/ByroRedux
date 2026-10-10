# #5518: EXT-D1-2026-10-09-03: The legacy LOD quad index is scanned under the terrain layout table, but the object ring consumes it under its own scheme table. #5423's "a future title adopting one scheme" rationale is applied to only one side

**Labels**: bug, game:fnv, game:fo3, low, terrain-exterior

**Source**: `docs/audits/AUDIT_EXTERIOR_2026-10-09.md` — finding `EXT-D1-2026-10-09-03` · suite `/audit-suite --preset streaming-deep` (2026-10-09) · HEAD `3bcf6c8e8`

- **Severity**: LOW (discipline; correct today because both tables map only `Fallout3NV`)
- **Dimension**: EXAL boundary discipline / Distant LOD
- **Location**:
  - `byroredux/src/streaming_helpers.rs:115-125`: the scan is gated on `terrain_lod_layout(..) == FalloutLegacy`.
  - `byroredux/src/cell_loader/object_lod.rs:200-216`: the `ObjectLodScheme::FalloutLegacyBlocks` arm reads `input.legacy_lod_quads...unwrap_or_default()`.
- **Status**: NEW (introduced by `b3e679dba`)
- **Tier Violated**: no-render-time-fallback (table-shape rule: the gate and the consumer key on different tables)
- **Game Affected**: none today (structural)
- **Description**:
  - #5423 replaced `game == Fallout3NV` at the scan with the terrain table.
  - A title (or mod profile) whose object scheme is `FalloutLegacyBlocks` but whose terrain layout is not `FalloutLegacy` would never scan. The object ring's `unwrap_or_default()` would then select nothing, silently, with no warn: the "authored path in one ring, nothing in the other" split #5423 set out to remove.
- **Suggested Fix**:
  - Scan when either table selects the legacy family (`terrain_lod_layout == FalloutLegacy || object_lod_scheme == Some(FalloutLegacyBlocks)`).
  - Alternatively, log when a `FalloutLegacyBlocks` ring sees `legacy_lod_quads == None`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other walkers / record decoders / games / call sites)
- [ ] **TESTS**: A regression test pins this specific fix
