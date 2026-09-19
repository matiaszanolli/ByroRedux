# P3-LOOT: P3 loot transfer is unreachable: `transfer_loot` has no production caller; six loot tests fail red (count underflow)

- **Labels**: high,bug,gameplay,inventory
- **Filed**: 2026-09-19, follow-up to the #4458 fix session (post-/audit-character)
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/4464

---

Discovered 2026-09-19 while fixing #4458: repairing the bin crate's compile break (`eb3784309`) allowed the binary-crate test suite to run for the first time since the P3-closure commits — and six loot tests fail, because the container/corpse loot transfer they test is **dead code in production**.

**Evidence**

- `cargo check -p byroredux --bin byroredux` warns on all three of the P3 loot primitives:
  - `LootSelection` is never used (`byroredux/src/inventory.rs:513`)
  - `LootOutcome` is never constructed (`byroredux/src/inventory.rs:524`)
  - `transfer_loot` is never used (`byroredux/src/inventory.rs:541`)
  i.e. the native container browser / corpse-loot UI was supposed to call `transfer_loot` (Take / Take All) and nothing does. The only live loot path is `pickup_loot` (loose single-item placements).
- The six red tests (all confirmed pre-existing on compile-repaired pristine HEAD, byte-identical):
  - `interaction::tests::lethal_combat_then_physical_activation_loots_corpse_through_its_collider`
  - `interaction::tests::physical_activate_takes_container_inventory_once`
  - `inventory::tests::container_loot_preserves_stacks_instances_equipment_and_activation`
  - `inventory::tests::corpse_loot_clears_source_equipment_and_emits_each_unequip_once`
  - `npc_spawn::tests::prebaked_race_skin_remains_intrinsic_through_equip_and_corpse_loot`
  - `save_io::round_trip_tests::container_and_corpse_loot_survive_encoded_live_overlay`
- One failure exposes a real arithmetic bug: `container_loot_preserves_stacks…` asserts against an expected stack carrying `count: 4294967295` (a `u32` underflow of `-1` — an authored `count: -1` reaching a `as u32` cast somewhere in the transfer/stack path) at `byroredux/src/inventory.rs:1994`.

**Impact**

Main's binary-crate suite is red (6 of these 8 outstanding failures), and the P3 "container + corpse loot through the native UI" feature is unreachable for players — the same silent-absence class as #4458, but on the Take/Take-All path. The tests encode the intended behavior and were never executed before because the bin crate could not compile in the environment that authored them (see the toolchain note in the compile-break issue).

**Suggested Fix**

1. Wire the native container browser's Take / Take All (and the corpse-loot activation path) to `transfer_loot` — that is the missing caller the API was built for.
2. Fix the `-1 → 4294967295` count underflow (guard authored counts the way `build_player_template_for` does: `count.max(0)`, or resolve the -1-means-one convention deliberately).
3. Get the six tests green; they are the spec for both fixes.

## Completeness Checks
- [ ] **SIBLING**: Once wired, check the pickup path (`pickup_loot`) and the transfer path share the theft/ownership rule rather than re-deriving it
- [ ] **TESTS**: The six red tests pass; one new test pins the negative-count handling
