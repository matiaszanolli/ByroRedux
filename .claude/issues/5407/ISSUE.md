# #5407: SCR-D1-2026-10-08-01: `AddRaceSpells`/`RemoveRaceSpells` lower a *script-defined* helper by name and discard any receiver, so `x.AddRaceSpells()` on any object lowers onto the player

**Labels**: medium,scripting,bug,game:skyrim
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5407

**Source**: `docs/audits/AUDIT_SCRIPTING_2026-10-08.md` — `SCR-D1-2026-10-08-01` (HEAD `00f580e09`)

**Publish note**: Validated at HEAD: `prim_race_spells_arity_ok` accepts both `Ident` and `MemberAccess` callees and `lower_race_spells` always emits `ActorRef::Player`.

- **Severity**: MEDIUM. This is latent: the skill escalation (emit on an unmodeled term) would make it HIGH, but the
  runtime is currently a no-op on the player, per CHAR-2026-10-08-D4-02.
- **Dimension**: Recognizer Chain
- **Untrusted-Input**: Yes (`.pex` from possibly-modded archives)
- **Location**: `crates/scripting/src/translate/effects.rs:1444-1495` (`lower_race_spells`,
  `prim_race_spells_arity_ok`, both `prim_*`).
- **Status**: NEW. CHAR-2026-10-08-D4-02 covers the runtime *target* (the player never receives `RaceSpells`). This
  finding covers the lowering contract.
- **Description**: neither name is an `actor.psc` native. They are functions defined on MQ101QuestScript. The primitive
  lowers any unqualified call by name, without checking that the calling script's own definition is the vanilla body. It
  also accepts any `MemberAccess` receiver ("The receiver is NOT distinguished") and always emits `ActorRef::Player`. A
  mod script that defines its own `AddRaceSpells()` with other semantics, or calls `SomeAlias.AddRaceSpells()` on
  another script type, is lowered as a re-apply/clear of the player's racial spells.
- **Impact**: none today. Once CHAR-D4-02 stamps the player's `RaceSpells`, a mod's same-named helper would add or remove
  the player's racial spells and their constant modifiers.
- **Related**: CHAR-2026-10-08-D4-02, #4415.
- **Suggested Fix**: accept only the unqualified spelling, and only when the calling script defines that function
  argless (the `ScriptSource` has the function table). Decline every member-call spelling.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other primitives that lower a script-defined (non-native) helper by name)
- [ ] **TESTS**: A regression test pins this specific fix
