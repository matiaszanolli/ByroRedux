# #4318 SCR-D5-2026-09-14-01: `Faction.SetEnemy` lowers `abSelfIsNeutralToOther` / `abOtherIsNeutralToSelf` and then discards them — `true` flags are recorded as mutual hostility

**Labels**: high,scripting,combat,bug,game:skyrim
**Source**: `docs/audits/AUDIT_SCRIPTING_2026-09-14.md`

- **Severity**: HIGH (domain table: recognizer emits a component on an unmodeled term instead of declining; today's sink has no reader, see Impact)
- **Dimension**: Recognizer-Chain Soundness
- **Untrusted-Input**: No
- **Location**: `crates/scripting/src/translate/effects.rs` `Effect::SetEnemy` variant (doc `abModifyPlayer, abModifyEnemy`) and `prim_set_enemy`; `crates/scripting/src/fragment/effects.rs` `Effect::SetEnemy { faction, other_faction, .. }` dispatch arm; `crates/scripting/src/combat.rs` `FactionRelations::set_enemy` doc
- **Status**: NEW (no issue mentions `SetEnemy`)
- **Description**: The real signature is `SetEnemy(Faction akOther, Bool abSelfIsNeutralToOther, Bool abOtherIsNeutralToSelf)`. A `true` flag makes that direction *neutral* instead of enemy. `prim_set_enemy` captures the flags under invented names (`modify_player` / `modify_enemy`, dead-code fallback `unwrap_or(true)` behind an exact arity check). The dispatch arm matches with `..` and pushes an undirected hostile pair regardless. This is the #3487 `MoveTo` lesson in reverse: an argument whose non-default value changes the call's meaning is accepted and ignored, where the rule is to decline.
- **Evidence**: Dim 5 extracted `scripts\faction.pex` from Skyrim SE `Skyrim - Misc.bsa` and parsed it with `byroredux_pex::parse`. The orchestrator re-ran the probe and got `Faction.SetEnemy(Faction akOther, Bool abSelfIsNeutralToOther, Bool abOtherIsNeutralToSelf) flags=0x2 [native]`. A raw string-table scan finds the neutral names and no `abModifyPlayer` / `abModifyEnemy`. The only pinning test, `lowers_mq101_set_enemy`, uses `false, false`; no test covers `true` or non-literal flags.
- **Impact**: `F.SetEnemy(G, true, true)` means "make F and G mutually neutral" but yields `FactionRelations::is_enemy(F, G) == true`, which is the wrong-predicate corruption the invariant forbids. Blast radius today: `FactionRelations` has no production reader. The orchestrator confirmed this: the `crates/sdk` / `mod-runtime` hits are the different `FactionRelationship` type. The corruption goes live as soon as ambient hostility reads it. MQ101's own calls pass `false, false` and are correct. Frequency of non-false flags in vanilla or mods was not measured.
- **Related**: #3487; SCR-D5-2026-09-14-02 (same invented-signature class)
- **Suggested Fix**: Accept only the literal default shape (both flags `false`) and decline otherwise, or model the neutral direction explicitly. Rename the fields to the Papyrus names, fix the `combat.rs` doc, and add decline tests for `true` and non-literal flags.

_Source: `docs/audits/AUDIT_SCRIPTING_2026-09-14.md`_

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other primitives / spawn paths / walkers)
- [ ] **TESTS**: A regression test pins this specific fix
