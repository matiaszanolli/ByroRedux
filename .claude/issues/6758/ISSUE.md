# CHAR-2026-11-07-M-03: ObScript RemoveFromAllFactions (FO3/FNV, 0x1214) clears FactionRanks directly and never records the ScriptedFactionChanges ledger

**Issue**: #6758
**Labels**: bug, medium, scripting, game:fnv, game:fo3, character
**Source audit**: `docs/audits/AUDIT_CHARACTER_2026-11-07.md` (finding M-03, Dim 4). Game: FO3, FNV.

## Description
Every scripted faction writer routes through the `ScriptedFactionChanges` ledger (recorded by reference FormID, replayed at spawn and load). Papyrus goes through `Effect::ChangeFaction`, which `flush_faction_changes` (`fragment/effects/deferred.rs:1027-1075`) applies and records, including a `FactionChange::RemoveAll` arm. ObScript `AddToFaction`, `RemoveFromFaction` and `SetFactionRank` also route through it (`obscript_effects/actor_state.rs:10-12`, `:143-151`). The one exception is the FO3/FNV `RemoveFromAllFactions` handler, which clears the component directly.

## Location
`crates/scripting/src/obscript_more/tail.rs:396-408` (handler), `:822` (registration `Command { id: 0x1214, games: Games::Fallout, .. }`).

## Evidence
```rust
// tail.rs:397-407
Some((_, actor)) => {
    if let Some(mut ranks) = world.query_mut::<FactionRanks>() {
        if let Some(factions) = ranks.get_mut(actor) { factions.0.clear(); }
    }
    done("RemoveFromAllFactions")
}
```
The only test (`obscript_more/tests.rs:662`) asserts the live component is empty; `ScriptedFactionChanges::record` appears nowhere in `obscript_more/`. Contract: `crates/scripting/src/faction_changes.rs:3-13`, `:55-98`. Not a numeric finding; behaviour per GECK *RemoveFromAllFactions*.

## Impact
On the next cell eviction/respawn or save-load the actor is re-derived from its record and the removed memberships return (ambient hostility, theft exemption, `GetFactionRank`/`GetInFaction` conditions). The player is affected through `reset_player_factions_to_record`. How often vanilla scripts call it is not measured.

## Related
#5919 (closed ledger-reload fix), #5058, `b3556ef6c`.

## Suggested Fix
Route the command through `Effect::ChangeFaction` with `FactionChange::RemoveAll`, as the Papyrus path does, instead of mutating the component in the handler. Add a ledger assertion beside the `:662` test.

## Completeness Checks
- [ ] **SIBLING**: every other direct `FactionRanks` write in `obscript_*` / `fragment/` goes through the ledger
- [ ] **LOCK_ORDER**: if the handler's query scope changes, lock acquisition order is preserved
- [ ] **TESTS**: a test asserts the ledger entry, not only the live component
