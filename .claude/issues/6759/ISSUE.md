# CHAR-2026-11-07-M-04: FactionReputation is mutated in production by ObScript and the SDK, but the save registry exempts it as having no mutation site

**Issue**: #6759
**Labels**: bug, medium, game:fnv, save-load, character
**Source audit**: `docs/audits/AUDIT_CHARACTER_2026-11-07.md` (finding M-04, Dim 5). Game: FNV. Registration fix belongs to the save audit's domain.

## Description
The registry-completeness guard accepts an exemption only when the component has no live mutator ("forward-latent ... Register the moment a real system starts inserting it", the guard's own doc at `registry_completeness_tests.rs:440-444`). `adjust_standing` does `standings.insert(player, Default::default())` on first use and then `add_fame/add_infamy/set_*` for the three FNV reputation ObScript commands (`AddReputation`, `AddReputationExact`, `SetReputation`), and the SDK reputation command does the same. `FactionReputation` is nowhere in `build_save_registry`; the debug-server projection states it is "not part of the save serialization surface".

## Location
`byroredux/src/save_io/registry_completeness_tests.rs:476` (the `NOT_SAVED_BY_DESIGN` row); mutators `crates/scripting/src/obscript_effects/mod.rs:229-243` (`adjust_standing`; callers `obscript_effects/actor_state.rs:187`, `obscript_more/tail.rs:522`, `obscript_more/actions.rs:185`) and `byroredux/src/extensions/commands.rs:678-686` (SDK `AddFame`/`AddInfamy`).

## Evidence
`registry_completeness_tests.rs:476`: `("FactionReputation", "forward-latent: no production insertion or mutation site exists yet")` against `obscript_effects/mod.rs:230-233`: `let mut standings = world.query_mut::<...FactionReputation>()?; if standings.get_mut(player).is_none() { standings.insert(player, Default::default()); }`. Not a numeric finding.

## Impact
On FNV, reputation earned through quest-script `AddReputation` resets to Neutral on save/load, and `GetReputation`/`GetReputationThreshold` CTDAs (573/575, wired) then read 0, flipping dialogue and quest branches. No log or test notices; the guard that exists to catch this is satisfied by a stale reason string. `charal.md` section 4.5 and `components.rs:195-197` claim the component rides the save machinery.

## Related
#4107 (closed; it called this row correctly forward-latent at the time, the premise has since been overtaken), #6151 (created `adjust_standing`, not a save fix).

## Suggested Fix
Register `FactionReputation` for save (serde under the `save` feature, FormId-keyed REPU entries like `FactionRanks`), delete the exemption row and add a round-trip test; or, if deliberately transient, correct the reason and the `components.rs` doc.

## Completeness Checks
- [ ] **SIBLING**: re-check every other `NOT_SAVED_BY_DESIGN` row's "no mutation site" claim against production mutators
- [ ] **TESTS**: a save/load round-trip test pins the reputation standing; `FORMAT_MAJOR`/baseline handled if the shape changes
