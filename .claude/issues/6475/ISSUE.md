# NIFAL-D4-2026-10-27-01: Three comments still say legacy furniture markers can never resolve to Sleep

Labels: low, nifal, gameplay, documentation, doc-rot, game:oblivion, game:fo3, game:fnv
Source: docs/audits/AUDIT_NIFAL_2026-10-27.md

# NIFAL-D4-2026-10-27-01: Three comments still say legacy furniture markers can never resolve to Sleep

**Source**: `docs/audits/AUDIT_NIFAL_2026-10-27.md` finding `NIFAL-D4-2026-10-27-01` (severity LOW, status NEW, CONFIRMED against HEAD `fa6be4a6b`).
**Location (re-mapped to current paths)**: `byroredux/src/systems/eat_sleep.rs:532-540` (comment + debug log text), `byroredux/src/cell_loader/references/attach.rs:66-68` (the `_ =>` arm of `furniture_component`), `crates/core/src/ecs/components/furniture.rs:23-30` (`FurnitureMarkerKind` doc).

## Description
0c6cd46dc taught `imported_furniture_marker` (`crates/nif/src/import/mod.rs` ~:392-404) to map a legacy `position_ref_1` of 1..=10 to `animation_type` 2 (Sleep); the source for that range is the CS *GetFurnitureMarkerID* page ("1 - 10 for beds"). The same commit rewrote the `eat_sleep.rs` module doc and the `FurnitureMarker.animation_type` field doc. Three nearby statements still describe the old behaviour:
- `eat_sleep.rs:532-536`: "on FO3/FNV this is not 'no bed found' but 'no bed can EVER resolve': legacy markers author animation_type 0 and the translate boundary reads them all as Sit ... until Phase C decodes furnituremarkerNN"; the debug log text (537-540) repeats it ("legacy cells author none (animation_type 0 over-match, #5390)"). This contradicts the module doc 20 lines above.
- `attach.rs:66-68`: "0 = legacy (Oblivion/FO3/FNV, no AnimationType authored at all)".
- `furniture.rs:23-30`: "the position kind is implied by the referenced `furnituremarkerXX.nif`, which isn't resolved at this layer ... A real per-marker decode is deferred to Phase C".

## Evidence
Verified at HEAD: the quoted comments are present verbatim at the three sites.

## Impact
None at runtime. A live debug log misreports the decision whenever the sit fallback fires on a legacy cell, and a future auditor/fixer would conclude the bed decode is still missing.

## Related
#5390 (closed; the original Sleep-unreachable bug), 0c6cd46dc.

## Suggested Fix
Reword the three sites to match the module doc: legacy beds (1-10) resolve to Sleep at the importer; every other legacy marker is the v0 Sit default. Change the log text to "no Sleep marker in range".

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the NIFAL parser->`Material` boundary - never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.

