# FNV-SD-03: XESP enable-parent children spawn in the wrong initial state for 56% of FNV children, and nothing cascades a parent's runtime enable/disable

Labels: medium,esm-plugin,gameplay,bug,game:fnv,legacy-compat

**Severity**: MEDIUM (incorrect world state with a workaround; arguably HIGH for quest-staged content) · **Dimension**: 1 (reference gating) · **Game**: FNV (FO3 and Oblivion share the code, see Related)

## Description
A non-inverted XESP child is visible iff its parent is enabled. The loader assumes every parent is enabled, so every non-inverted child spawns and every inverted child is skipped. FNV authors **disabled "marker" parents** that quests enable later, with the staged content parented to them. The parent's own `Initially Disabled` flag (0x800) is decoded and honoured for the parent (#4813), but the child never consults it. Nothing cascades a parent's runtime `Enable`/`Disable` to its children either.

## Evidence
Location: `byroredux/src/cell_loader/references/mod.rs:500-517` (skip only when `EnableParent::default_disabled()`), `:640-649` (`reference_is_disabled(..., placed_ref.initially_disabled && placed_ref.enable_parent.is_none())`: the child's own flag is ignored when it has a parent), `crates/plugin/src/esm/cell/placed_ref.rs:466-490` (`default_disabled(self) = form_id != 0 && inverted`, "interim predicate ... long-term fix is a two-pass loader"), `crates/scripting/src/fragment/state.rs:85-95` (`AuthoredDisabledReferences` repeats the rule, so `IsDisabled`/`IsEnabled` are wrong the same way), `crates/scripting/src/obscript_actions.rs:25-28` ("the runtime has no enable-parent ledger to consult"). Re-confirmed present at HEAD.

Census (global FormID -> flag table over interior, exterior and persistent cells; parents all resolve):
- 3,337 XESP children in FalloutNV.esm. **1,876 spawn although their parent starts disabled** (exterior 768, persistent 354, interior 754; 540 of them actors) and **72 are skipped although they should be visible** (inverted child of a disabled parent). 17 of the 1,876 have a parent that itself carries an XESP.
- Raw check: parent `0015D98C` (`VHDBattleEffectsMark...`) is `REFR flags=0xc00` (Persistent + Initially Disabled); its children `0015E4AF`/`0015E4B5` carry non-inverted XESP: fires, `ImpactScorch02` decals, `Vault87Blood01` and `VHDDeadNCRTrooper` corpses of the Hoover Dam battle, all visible from a fresh start. Also the Strip light beams under `VOVWestsideLightStar...` (`0013A9ED`), Legion recruit corpses under `00152407`, smoke/fire FX under `0016A270`.
- Goodsprings area: of 36 XESP children within radius 12 of (-17, 0), 24 spawn wrongly. The Prospector saloon has 0 XESP children.
- The child's own Initially-Disabled flag does not cover it: the loader drops it whenever an XESP exists, and 784 of the 1,876 carry it.

## Impact
Aftermath dressing, battle scenes, quest-gated NPCs and effect sets appear from the start of the game. Because `Enable()` on a parent does not cascade either, fixing only the initial state would hide them permanently when a script later enables the parent. Fix both.

## Related
Closed #471 / #349 (intentional interim), #4813 (explicitly leaves XESP children to the heuristic), SCR-SD-03, `reference_enable_gate_tests.rs`. Twins: FO3-SD-07 (6,818 of 12,236 wrong, 56%), OBL-SD-02 (Oblivion ~28% single-level to ~56% full-chain of 9,661).

## Suggested Fix
Build a FormID -> (initially_disabled, enable_parent) table once per load order (the data `install_authored_disabled_references` already walks), evaluate parent state recursively (cycle-capped) with `ReferenceEnableState` overrides, and cascade a script `Enable`/`Disable` of a parent to its children.

Source: docs/audits/AUDIT_FNV_2026-10-19.md (area-scoped, streaming-deep)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / the sibling games' paths (FO3, FNV, Oblivion share this path)
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the parser / spawn boundary, never pushed into shaders or the renderer
- [ ] **TESTS**: A regression test pins this specific fix (real-data `--ignored` test where the defect is a data census)
