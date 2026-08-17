# TD3-2026-08-16-01: docs/feature-matrix.md states native menus are "Not planned" while the engine ships a three-page native game menu

**Issue**: #2975
**Severity**: MEDIUM
**Dimension**: 3 — Stale Documentation & Comments
**Labels**: `medium,tech-debt,documentation`
**Source report**: `docs/audits/AUDIT_TECH_DEBT_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-08-16.md` (Dimension 3 — Stale Documentation & Comments). Effort: trivial.

**Location**: `docs/feature-matrix.md`:207, :224
**Age**: row written `58e14e04`, 2026-07-25; falsified 2026-08-15/16

## Description

The UI table's last row reads:

```
| Native menu reimplementation | Not planned; preserve SWF compatibility through Ruffle profiles |
```

That is a **policy statement**, and it is now false:

- `crates/debug-ui` ships `GameMenuPage::{Pause, Settings, Inventory}` with `draw_game_menu`
- `byroredux/src/inventory.rs` (546 LOC, *"Native inventory presentation and player-facing equipment mutations"*) supplies its `InventorySnapshot` and consumes its `InventoryAction`
- `byroredux/src/settings_io.rs` (334 LOC, *"Persistent user settings for the native menu"*) persists the Settings page

This is doc rot in the direction that costs most: **documented capability is lower than reality**, so a reader — including the next auditor — concludes that ~900 LOC of shipped, scheduled code should not exist.

The same section is headed *"What Doesn't Work Yet (live gaps as of 2026-08-12)"* and has **no gameplay/combat rows at all**, four days after the P2 melee core landed.

## Evidence

- `docs/feature-matrix.md`:207 — the row above
- `crates/debug-ui/src/panels.rs`:185-190 — `enum GameMenuPage { Pause, Settings, Inventory }`
- `crates/debug-ui/src/lib.rs`:279-289 — `open_inventory` / `close_game_menu`; :348 — `panels::draw_game_menu`
- `byroredux/src/inventory.rs`:255 — `snapshot(world) -> Option<byroredux_debug_ui::InventorySnapshot>`; :319 — `apply_action(world, InventoryAction)`
- `byroredux/src/settings_io.rs`:1-7 — module docstring
- `ROADMAP.md`:562-599 documents the whole P0/P1/P2 slice and is current to 2026-08-16, so this is a **matrix-only lag**, not a project-wide one

## Impact

The matrix is named in `.claude/commands/_audit-common.md` as one of eight authoritative reference docs ("prefer them over re-deriving facts from source"). An auditor who obeys that instruction is told the native menu is out of scope **by policy**.

The UI section also mis-frames `/audit-ui`'s subject area.

## Suggested Fix

Replace the row with the real state — native egui game menu with Pause/Settings/Inventory pages shipped 2026-08-15/16; Scaleform remains the compatibility target for authored Bethesda menus.

Add a Gameplay/Combat section covering the P0–P2 slice, and refresh the gap-section date.

## Related

- #2961 (OPEN — the same file has no character/progression rows; sibling gap, different subject)
- #2729 (OPEN — ROADMAP M48 input-routing row)

## Completeness Checks
- [ ] **SIBLING**: #2961's character/progression gap addressed in the same pass — same file, same sweep
- [ ] **DATE**: The "live gaps as of …" header date refreshed, not just the row
- [ ] **DIRECTION**: Checked for other rows understating shipped capability, not only this one
- [ ] **PATH-GATE**: `.claude/commands/_audit-validate.sh` still passes

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state —
query `gh issue view 2975 --json state` when live state is needed.*
