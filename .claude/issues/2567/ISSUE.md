# OBL-D3-01: Creature placements (Oblivion ACRE, and cross-game ACHR->CREA) never route through the actor spawn pipeline

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2567
**Finding ID**: OBL-D3-01

**Severity**: MEDIUM
**Dimension**: ESM Record Coverage (live path)
**Location**: `byroredux/src/cell_loader/references/mod.rs:485`, `byroredux/src/cell_loader/exterior.rs:174,294,341`, `byroredux/src/cell_loader/load.rs:404-418`, `crates/plugin/src/esm/records/dispatch_actor.rs:42-49`
**Status**: NEW — not a regression of #396 (which fixed CREA/ACRE *parsing* into the statics fallback and explicitly treated the static-mesh render as its acceptance bar)

## Description
NPC_ and CREA are parsed into two disjoint maps (`index.npcs`/`index.creatures`), both typed `NpcRecord`. Every runtime call site in `byroredux/src/cell_loader/` that decides "is this REFR an actor" checks only `index.npcs` — `index.creatures` is never consulted anywhere under `byroredux/src/` (confirmed by exhaustive grep). A placed `CREA` base form falls through to the generic static-mesh instance path, which only animates via an *embedded* NIF controller clip (#544) — never via external `.kf` skeletal locomotion/idle, the mechanism NPC_ actors use.

## Evidence
Confirmed directly: `grep -rn "index.creatures" byroredux/src/` returns zero hits; `index.npcs` is checked at `exterior.rs:174,294,341,1160` and `load.rs:408`.

## Impact
Oblivion dungeons, Ayleid ruins, the Arena, and wilderness encounters — a large fraction of Oblivion's placed-actor content — render creatures frozen in bind pose, easy to mistake for a KF-importer gap rather than a spawn-routing gap. Cross-game (any FO3+ master with `ACHR`→`CREA` hits the same gap) but Oblivion is highest-density since `ACRE` is dedicated and ubiquitous there.

## Suggested Fix
Thread `index.creatures` alongside `index.npcs` into `load_references`/`load_references_budgeted` and the exterior call sites; extend actor-detection to check both maps (both already share `NpcRecord`); add a no-race fallback to the runtime FaceGen path since creatures typically reference no RACE.

## Completeness Checks
- [ ] **TESTS**: A regression test spawns a placed `CREA`/`ACRE` REFR and confirms it routes through the animated-actor spawn pipeline
- [ ] **SIBLING**: Confirm all actor-detection call sites (interior + exterior + streaming) check both maps consistently
