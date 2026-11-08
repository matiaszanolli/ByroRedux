# ESM-D6-03: `StringTableSet::resolve` is table-agnostic, so an id present in two tables resolves to the wrong one

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/6798
Labels: low, esm-plugin, bug, game:skyrim
Source: docs/audits/AUDIT_ESM_2026-11-08.md

_Filed from `docs/audits/AUDIT_ESM_2026-11-08.md` (audit HEAD 3500d2737)._

- **Severity**: LOW (1 hit in Skyrim's shipped masters, measured)
- **Dimension**: 6
- **Location**: crates/plugin/src/esm/strings_table.rs:447-453 (`resolve`: STRINGS, then DLSTRINGS, then ILSTRINGS), called from records/common.rs:211
- **Status**: NEW
- **Description**: the three companion files are separate id namespaces. The reader picks the first table that has the id regardless of which sub-record asked. Bethesda content mostly avoids overlap by sharing one counter, but not always.
- **Evidence**: id collision census over the shipped `_en`/`_english` tables (probe `collide`): Skyrim.esm family 1 (`dragonborn_english`, id 148), Fallout4_en 2 (ids 224439, 224441), Starfield 0, FO76 0. For Dragonborn id 148: `.STRINGS` = "Quick Test Cell", `.DLSTRINGS` = "Protection spells like Stoneflesh are 2.5 times as strong if...". Parsing Dragonborn.esm gives `PerkRecord { editor_id: "DLC2MageArmor50NPC", description: "Quick Test Cell" }` (the CELL FULL string wins over the PERK DESC string). The FO4 pair collides with stray Japanese text in `Fallout4_en.DLSTRINGS`, benign in practice.
- **Impact**: one wrong perk description in vanilla Dragonborn; modded plugins can collide more (each mod allocates ids per table). No crash, wrong text.
- **Suggested fix**: pass the preferred table to the lookup (DESC and long-text sub-records -> DLSTRINGS, INFO `NAM1` -> ILSTRINGS, everything else -> STRINGS) and keep the current order only as the fall-through. The mapping must come from xEdit/CK behaviour (not guessed); the DESC/INFO split is already implied by the file-purpose doc on `StringTableSet`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related decoders / other games' arms
- [ ] **LOCK_ORDER**: n/a unless a RwLock scope changes
- [ ] **TESTS**: A regression test pins this specific fix (real-data pin where an install exists)
