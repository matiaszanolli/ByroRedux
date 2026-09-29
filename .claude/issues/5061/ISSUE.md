# #5061 — PERF-D7-2026-09-29-02: GearImportLoader opens a third private archive set on the main thread at the first mid-life equip and imports worn NIFs bypassing NifImportRegistry

**Labels**: low, bug, performance, import-pipeline

**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-09-29.md` — finding `PERF-D7-2026-09-29-02`

**Severity**: LOW

**Dimension**: Streaming & Cells

**Location**: `byroredux/src/npc_spawn/loot_appearance.rs:549-600` (`GearImportLoader::step`)

**Status in report**: NEW (`0182fc5e8`)

## Description

the loader mirrors `LootAppearanceLoader` by lazily calling `build_texture_provider` and `build_material_provider`. The first equip of an item the wearer never spawned wearing therefore re-opens every mesh and texture archive (headers and file tables) on the main thread during gameplay. The loader then keeps a third resident copy of those tables and a third BGSM cache. Each import extracts, parses and imports on the main thread (one NIF per frame) with no `NifImportRegistry` lookup, so the same armor on a second wearer is parsed again.

## Impact

a one-time first-equip hitch (unmeasured; scales with archive count) plus duplicated archive-index memory for the session. Event-driven, not per frame.

## Suggested Fix

share one provider set across the corpse and gear loaders (a World resource or the streaming state's `Arc<TextureProvider>`), and route the import through `NifImportRegistry`.

Validated at HEAD 9fcfdc3fc: `GearImportLoader::step` (`byroredux/src/npc_spawn/loot_appearance.rs`) still lazily calls `build_texture_provider` / `build_material_provider` and imports via `load_nif_bytes_with_skeleton` with no `NifImportRegistry` lookup.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (`LootAppearanceLoader` shares the same provider-construction pattern)
- [ ] **TESTS**: A regression test pins this specific fix
