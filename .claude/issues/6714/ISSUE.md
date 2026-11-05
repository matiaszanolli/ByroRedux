# RF7-2026-11-05-04: `render` depends on `systems`, `cell_loader`, `npc_spawn`, `inventory` and `player_body` for small shared items

Labels: low,tech-debt,bug,renderer

**Severity**: LOW (no runtime effect; maintainability)
**Dimension**: Refactor backlog, item 7
**Status**: NEW. Part of #6680

## Location
- `PlayerEntity` (`byroredux/src/systems/character.rs:44`), used at `byroredux/src/render/mod.rs:284` and by `cell_loader`
- `RT_ABSOLUTE_PRECISION_CEILING` (`byroredux/src/cell_loader/references/mod.rs:166`), used at `byroredux/src/render/fog_volumes.rs:165`
- `DEFAULT_TOD_HOURS` and `night_factor_for_hour` (`byroredux/src/systems/weather.rs`), used by `byroredux/src/render/lights.rs`, `byroredux/src/render/sky.rs`, `byroredux/src/render/water.rs`
- Markers `HiddenFirstPerson`, `NpcAppearanceHidden` and `PickedUp`, each used 3 times in `byroredux/src/render/`

## Why
The render collector reads these constants, one resource and three markers, but each lives in a higher-level module. `DEFAULT_TOD_HOURS` is already an alias of `FB_TOD_HOURS` from `env_translate`.

## Change
Move the resource and markers into `components`, move the constant down, and read `FB_TOD_HOURS` directly.

## Constraints
Move items verbatim. Run `grep -rn include_str` for each source file first. This does not propose splitting the binary crate. `acquired_in` recognises `PlayerEntity::get` by name, and `byroredux/src/systems/character.rs` has several `include_str!` readers.

## Acceptance Criteria
- `byroredux/src/render/` production code no longer names `cell_loader`, `npc_spawn`, `inventory` or `player_body`
- `cargo test -p byroredux --bin byroredux render` passes
- Non-timing tokens of the `bench:` line match a baseline build of the parent commit

## Completeness Checks
- [ ] **LOCK_ORDER**: TypeId-sorted lock acquisition is preserved wherever a RwLock scope moves
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A test or guard pins the change, and any guard that scans the old spelling was extended in the same commit

---
Source: refactor review of 2026-11-05 (at `16d9e18f3`), re-measured at `02f46f95d`. Counts come from line and regex scripts. Fix with: `/fix-issue <this issue>`.

_Immutable snapshot of the issue as filed. GitHub is authoritative for current state (`gh issue view 6714 --json state`)._
