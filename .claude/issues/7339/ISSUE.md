# #7339: GP-03: A Near Editor Location Sandbox now snaps the actor into a chair near its editor placement, from anywhere in the cell

- **Severity**: MEDIUM (the same class, #5726, was filed LOW; keep that calibration if preferred)
- **Dimension**: 5 (NPC spawn, AI package selection, locomotion)
- **Location**: `byroredux/src/systems/sandbox.rs:352-365` (the new `centre`), `:405` (assignment), `:202-219` (`apply_seat_assignments` snap); introduced by #7262 5ede7b032
- **Status**: NEW (regression-class sibling of closed #5726, which bounded the same seat-snap teleport for Eat/Sleep In-Cell; #7262 closed without this check)
- **Domain**: ai, gameplay · **Game**: FNV, FO3 (424 + 72 type-3 Sandbox packages, per the #7262 commit)
- **Trigger**: FNV or FO3, any cell with sit markers. A sandboxing NPC whose Sandbox package has PLDT type 3 (Near Editor Location) and whose position is far from its `EditorPlacement` when its seat pass first runs. The common displacements:
  - a fight or flee that ends, after which `suspend_ambient_behavior_for_combat` resets the winner and the first tick re-installs the Sandbox;
  - a handover from Travel, Follow, Wander or Flee to the Sandbox package.
  Any save state. An actor still standing at its authored spot is unaffected.
- **Description**: #7262 centres the seat search on `travel::package_home(.., near_editor_location, npc_g.translation)`, i.e. on the `EditorPlacement`. The pass then assigns the picked seat with no arrival test, and `apply_seat_assignments` writes the placement root's `Transform` straight to the seat. Sandbox has no walk leg. So an actor N units from its editor placement is teleported to a chair within `radius` of that placement, a jump of up to N + radius, instead of the old jump of at most `radius` (512 default) from where it stood. Eat/Sleep, which share the same snap, guard this exactly: "a picked marker beyond `ARRIVE_RADIUS` becomes the next KCC walk goal instead of a snap" (#5726). Travel, Wander and Guard, the other #7225/#7254 consumers, all walk to the placement. Sandbox is the only one that teleports.
- **Evidence**:
  ```rust
  // sandbox.rs:358-365
  let centre = super::travel::package_home(world, npc, behavior.near_editor_location, npc_g.translation);
  if let Some((seat_id, seat)) = pick_nearest_seat(centre, &scratch.seats, &reservations.0, radius) {
  // ... :405  scratch.assignments.push((npc, furn_e, seat));  -> :417 apply_seat_assignments
  // sandbox.rs:215  t.translation = seat.translation;   // unconditional snap
  ```
  The #7262 test `a_near_editor_location_sandbox_seats_near_the_editor_placement` puts the actor 4000 units from the chair that gets claimed and asserts only the reservation. It never asserts that the actor walks or that the jump is bounded.
- **Impact**: A visible NPC pop across the cell (up to the cell's extent), in front of the player, for every displaced type-3 sandboxer. It is also a mismatch with the GECK's own rule, since Sandbox's Wander Location is where the actor goes and then idles.
- **Related**: #7262, #5726, #7225, #7254, AIP-SIT-001/-002.
- **Suggested Fix**: Seat at the picked marker only when the actor is within the arrival radius of it (as `seat_at_marker` does). Otherwise leave the actor to walk toward the `EditorPlacement` with the same `cached_walk_step` plumbing Wander and Guard use, then seat. Add a test with the actor displaced beyond `ARRIVE_RADIUS` that asserts its `Transform` is not moved to the far chair.
- **Dimension agent ID**: GP-D5-01

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other action systems / movers / park paths / add paths named in the finding)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved and the system's `Access` row matches
- [ ] **TESTS**: A regression test pins this specific fix

**Found by**: `docs/audits/AUDIT_GAMEPLAY_2026-11-23.md`
