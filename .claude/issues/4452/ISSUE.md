# D2-03: D2-03: DerivedScope consumer contract honored by only one of the two live consumers (melee_damage_charal_bonus skips it)

- **Labels**: low,character,bug
- **Filed from**: docs/audits/AUDIT_CHARACTER_2026-09-19.md (/audit-character 2026-09-19)
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/4452

---

**Source**: `crates/core/src/character/derived.rs:124-132` — DerivedScope's own contract: "A consumer that computes a derived stat for an arbitrary entity checks this before trusting the result."

**Description**

The containment of the FO3/FNV player Health/AP deferral (the known-open #2937 family) rests on consumers consulting `DerivedScope`. One of the two production consumers does (`GetActorValue`, crates/scripting/src/condition.rs:505-533, checks `scope == ActorGeneral && kind == Absolute`); the melee-damage one does not (`melee_damage_charal_bonus`, byroredux/src/combat.rs:432-460, evaluates the MeleeDamage row with no scope check).

**Evidence**

Coincidentally correct today — the only row `melee_damage_charal_bonus` reads is ActorGeneral — but nothing enforces the contract, and the skipping consumer is the newer of the two.

**Impact**

Zero today. A future row evaluated through a copy of the melee-damage pattern (e.g. someone wiring FO4 Health for "the player" via the same helper) would apply a player-only formula to arbitrary actors with no error and no test failure — the exact leak DerivedScope exists to contain.

**Related**

#2937 (why the tagging matters); CHAR-D6-05/#2962 (consumer-drift reporting mandate).

**Suggested Fix**

Either add the (currently tautological) scope assert/filter in `melee_damage_charal_bonus`, or record in its doc comment that it reads a known-ActorGeneral row and must re-check if generalized.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other per-game tables, other doc sites making the same claim)
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed from `docs/audits/AUDIT_CHARACTER_2026-09-19.md` (finding D2-03, /audit-character 2026-09-19, HEAD `479163836`).*