# #7225: GAME-2026-11-22-02: A Travel or Wander package to *Near Editor Location* anchors on where the actor stands, not on its editor placement

**Labels**: medium,gameplay,ai,bug,game:oblivion,game:fnv,game:fo3

From `docs/audits/AUDIT_GAMEPLAY_2026-11-22.md` (report ID **GP-02**), audited at HEAD 2b40f76b6.

- **Severity**: MEDIUM
- **Dimension**: 5 (locomotion / package install)
- **Domain**: ai · **Game**: Oblivion, FNV (FO3 shares the PLDT type)
- **Trigger**: An actor moved away from its authored position by an earlier package, then handed a Travel or Wander package whose PLDT is *Near Editor Location* (type 3). Vanilla Oblivion:
  - `HorseGoHome` (0xc55fd, radius 1024, 13 users)
  - `SE45zReturnHome` / `SE45mReturnHome` (radius 512)
  - `MS38GlarthirGoHome`
  - `HirtelFleeFromKvatch`

  Totals: FNV 394 type-3 Travel packages (271 package edges); Oblivion 767 edges (157 Travel packages plus 415 Wander ones).
- **Location**: `byroredux/src/npc_spawn/ai_package.rs:462-483` (`from_package` Travel arm: `NearEditorLocation` leaves `target_form_id: None`); `byroredux/src/systems/travel.rs:148-159` (`resolve_destination`), `:352-357` (called with `home = current`); `byroredux/src/systems/wander.rs:390-393` (`home = transform.translation`)
- **Status**: NEW. A sibling of closed #5391 / #5500, which fixed exactly this for Eat/Sleep through `EditorPlacement`.
- **Description**: `TravelBehavior` carries only a radius and a reference FormID, so the PLDT *type* is lost at the `AmbientBehavior` boundary. For type 3 there is no reference: `resolve_near_reference_target` returns `None`, and `resolve_destination` falls back to `pick_wander_target(home = current, radius)`. A "go home" Travel therefore picks a point near the actor's *current* position (with radius 0, its current position exactly), arrives at once and stamps `Traveled`. The comment from f09ba4ee6 assumes "the actor is placed at its editor location". That holds only until an earlier package moves it.
- **Evidence**: `EditorPlacement` (`crates/core/src/ecs/components/eat_sleep.rs:55-66`) is documented as "the anchor of a 'near editor location' package, which must not follow the actor wherever an earlier package walked it". `spawn_placement_root` stamps it on every actor (`npc_spawn/resumable/mod.rs:381-388`), but only `systems/eat_sleep.rs:473-475` and `forcegreet.rs` read it; `travel.rs` and `wander.rs` never do. `from_package` already computes `location_anchor(package.location)` for the Travel arm, then discards it except for two radius special cases.
- **Impact**: ReturnHome / GoHome packages are silent no-ops: horses never return and quest NPCs do not go back to their posts. A type-3 Wander with a radius wanders around wherever it was installed.
- **Related**: #5391, #5500, f09ba4ee6, 61a1da1cd, #6662
- **Suggested Fix**: Carry the `EatSleepLocation` anchor `from_package` already derives into `TravelBehavior` and `WanderBehavior` (and Patrol/Guard/Sandbox as needed). Resolve `NearEditorLocation` to `EditorPlacement.translation` in `resolve_destination` and the wander `home`, with the same unresolved-anchor idle rule Eat/Sleep uses. Add a test that moves the actor and then installs a type-3 Travel.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other procedures / other transients / other inventory paths)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
