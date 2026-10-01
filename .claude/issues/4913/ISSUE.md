# 4913: EXT-D6-2026-09-27-03: Skyrim distant trees are never drawn — 386 `.btt` tree-LOD files and the `treelod` atlases go unconsumed

labels: bug, medium, game:skyrim, terrain-exterior
state: OPEN

**Severity**: MEDIUM (coverage hole)
**Dimension**: Distant LOD and trees
**Tier Violated**: n/a
**Game Affected**: Skyrim (LE/SE)
**Status**: NEW
**Location**:
- `byroredux/src/cell_loader/object_lod.rs` (`.bto` only).
- `docs/engine/exal-trees.md:83-87,307-315`.
- `docs/engine/exal.md:156`.
**Source**: `docs/audits/AUDIT_EXTERIOR_2026-09-27.md` (HEAD `0e0d35b96`)

## Description
- `Skyrim - Meshes1.bsa` ships 386 `.btt` (Tamriel 329, dlc2solstheimworld 24, …), 9 `.lst`, and the `textures\terrain\<ws>\trees\<ws>treelod.dds` atlases.
- Vanilla Skyrim `.bto` files carry no trees.
- No code or probe references `.btt`, `.lst` or `treelod`.
- FO4 and FO76 bake their trees into `.bto`, so the gap is Skyrim-only.
- exal-trees.md §7 claims `.bto`/`.btr` cover the distant tier, which is false for Skyrim.

## Impact
Skyrim forests stop at the full-detail radius, while the mountains behind them keep drawing.

## Suggested Fix
- Register the family: a scheme-table entry plus a probe counter.
- Correct both docs.
- Build an instanced-billboard consumer from `.btt` + `.lst` on the object ring's quad residency.

## Related
#3307

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other spawners / translate arms / games)
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the EXAL/WATAL translate boundary (`env_translate.rs`, `groundcover_translate.rs`, the `cell_loader` spawn sites) — never pushed into shaders/renderer, never re-derived at render time. See `/audit-exterior`.
- [ ] **TESTS**: A regression test pins this specific fix

