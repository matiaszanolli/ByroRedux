# NIFAL-D7-2026-09-21-01: Draugr combat clips are never installed on the cell-loader route — P2 combat takes silently no-op on --cell runs

**Labels**: medium, nifal, animation, game:skyrim, bug

**Severity**: MEDIUM · **Dimension**: Animation / controllers (P2 combat tail install path) · **Tier Violated**: none strictly (boundary correct; install-site coverage gap) · **Game Affected**: Skyrim
**Location**: `byroredux/src/cell_loader/load.rs:635-637` and `:1014-1015` (missing call) vs `byroredux/src/scene/world_setup.rs:1012-1017` (present); `byroredux/src/asset_provider/animation.rs:251` (`populate_draugr_combat_clips`)
**Source**: `docs/audits/AUDIT_NIFAL_2026-09-21.md` (CONFIRMED at `4dca737e2`)

### Description
`populate_draugr_combat_clips` is invoked only from the `--game` world-setup route (sole production caller `world_setup.rs:1017`; the `animation.rs` hits are `#[cfg(test)]`). The `--esm … --cell` route installs `populate_idle_clip_runtime` + `populate_skyrim_walk_clip` at both of its sites but never the combat family, while the shared spawn finalize still inserts the `DraugrCombatAnim` marker (`npc_spawn/resumable.rs:1075-1080`, gated only on race + skeleton) — so `combat_feedback_system` runs, finds no `DraugrCombatClips` resource, and silently no-ops. The P2 gate smoke script drives exactly this route (`docs/smoke-tests/p2-melee-core.sh` → `--esm $FIXTURE_ESM --cell BleakFallsBarrow01`, target `encdraugr01ambushmelee2hheadm06` / REFR `0x0383F7`), so the fixture doc's step-4 gate ("assert the death take actually started") cannot pass where it is designed to run.

### Evidence
`grep -rn populate_draugr_combat_clips byroredux/src` → one production call site; `cell_loader/load.rs:636`/`:1015` install the walk clip with no combat sibling.

### Impact
The headline P2 capability ("play one attack/hit/death animation family and spatial sound family") is inert on the `--cell` route — the route every current smoke test and the AGENTS.md usage examples use — while working on `--game`.

### Related
`docs/engine/p2-combat-anim-sound-fixture.md` §Wiring step 4

### Suggested Fix
Add `populate_draugr_combat_clips` beside the two `populate_skyrim_walk_clip` call sites in `cell_loader/load.rs` (already idempotent + game-gated), or fold the three clip installers into one helper so a future route cannot pick up two of three.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other readers, other gates)
- [ ] **TESTS**: A regression test pins this specific fix
