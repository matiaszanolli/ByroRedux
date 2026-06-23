# LC-D7-02: VWD / "Has Distant LOD" record-header flag (0x00010000) not parsed

**Issue**: #1731
**Source audit**: `docs/audits/AUDIT_LEGACY_COMPAT_2026-06-23.md`
**Severity**: LOW · **Labels**: low, legacy-compat, bug
**Dimension**: 7 (subsystem coverage) / 5 (EXAL LOD)
**Location**: `crates/plugin/src/esm/reader.rs:19` (only `FLAG_COMPRESSED = 0x00040000` decoded; `header.flags` stored but never masked against `0x00010000`)

## Description

The base-record header Visible-When-Distant / "Has Distant LOD" flag (`0x00010000`) is never read. The decoder captures only `FLAG_COMPRESSED` (0x00040000) plus TES4-file-header Localized (0x80) and Light-Master (0x0200) bits. `exal.md` §5.4 names this as the parser gap blocking full-model VWD culling.

## Evidence

`reader.rs:19` defines exactly one record `FLAG_*` constant. `header.flags` masked only against `FLAG_COMPRESSED` (`:533`), `0x80` (`:679`), `0x0200` (`:683`) — never `0x00010000`. `object_lod.rs` comment confirms full-model VWD cull deferred. Distinct from the `0x20` deleted-REFR tombstone (SKY-D4-01 / #1660).

## Impact

Without the flag the engine cannot decide which full-resolution base models to cull once their LOD stand-in shows, nor which records are LOD-eligible. Low severity: the LOD pipeline distance-gates by other means; the flag is a refinement, not load-bearing. Prerequisite for LC-D7-01's correctness.

## Related

LC-D7-01; SKY-D4-01 / #1660 (different flag, not a dup).

## Suggested Fix

Add `FLAG_VISIBLE_WHEN_DISTANT: u32 = 0x00010000`, expose it on the parsed record header, consume it in the LOD spawn path.
