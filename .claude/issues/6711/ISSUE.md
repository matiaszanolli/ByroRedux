# RF7-2026-11-05-01: `components` reaches up into `systems` and `asset_provider`

Labels: low,tech-debt,bug

**Severity**: LOW (no runtime effect; maintainability)
**Dimension**: Refactor backlog, item 7
**Status**: NEW. Part of #6680

## Location
- `byroredux/src/components/ai_anim.rs:146` uses `LOCOMOTION_WALK_SPEED` (`byroredux/src/systems/locomotion.rs:34`)
- `byroredux/src/components/terrain.rs:124-126` uses `LayerHeightSources` (`byroredux/src/asset_provider/terrain_height.rs:29`)

## Why
`components` should be the crate's lowest layer, but these three production references point upward. `components/environment.rs` also uses `crate::fog`, which has no `crate::` dependencies of its own, so that one is fine.

## Change
Move the constant and the struct down into `components`.

## Constraints
Move items verbatim. Run `grep -rn include_str` for each source file first. This does not propose splitting the binary crate.

## Acceptance Criteria
- `rg 'crate::(systems|asset_provider)' byroredux/src/components` finds no production reference
- `cargo test -p byroredux --bin byroredux` passes

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A test or guard pins the change, and any guard that scans the old spelling was extended in the same commit

---
Source: refactor review of 2026-11-05 (at `16d9e18f3`), re-measured at `02f46f95d`. Counts come from line and regex scripts. Fix with: `/fix-issue <this issue>`.

_Immutable snapshot of the issue as filed. GitHub is authoritative for current state (`gh issue view 6711 --json state`)._
