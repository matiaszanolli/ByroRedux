# Investigation: #228 — NiPathController / NiLookAtController not parsed

## Domain
nif (animation controllers)

## Current state
- Neither block name has a dispatch arm in `crates/nif/src/blocks/mod.rs::parse_block`.
- Oblivion/FO3/FNV/Skyrim-LE scenes that reference either type fall through to `NiUnknown` (with block_size recovery) — the wire fields are discarded.
- Both blocks are present in the Gamebryo 2.3 codebase and nif.xml (`docs/legacy/nif.xml:4345` and `:4617`), marked DEPRECATED at 10.2 and REMOVED at 20.5.

## Wire format (from nif.xml)
### NiLookAtController (inherits NiTimeController)
- 26-byte NiTimeControllerBase
- `Look At Flags: u16` (since 10.1.0.0 — 0 before)
- `Look At: Ptr<NiNode>` = `BlockRef` (4 bytes)

### NiPathController (inherits NiTimeController)
- 26-byte NiTimeControllerBase
- `Path Flags: u16` (since 10.1.0.0)
- `Bank Dir: i32`
- `Max Bank Angle: f32`
- `Smoothing: f32`
- `Follow Axis: i16` (0/1/2 = X/Y/Z)
- `Path Data: Ref<NiPosData>` (4 bytes)
- `Percent Data: Ref<NiFloatData>` (4 bytes)

## Scope
Parsing only, per finding severity. Actual ECS path-follower and
look-at constraint systems are noted as follow-ups — the issue
description asks for "parse controller blocks and introduce ... ECS
systems" but the audit marked it LOW and the blocks-first work is the
blocker for everything downstream (including just getting them out of
`NiUnknown` noise in nif_stats telemetry).

## Files touched
- `crates/nif/src/blocks/controller.rs` — new structs + parsers + tests
- `crates/nif/src/blocks/mod.rs` — import + 2 dispatch arms

2 files, no scope check needed.
