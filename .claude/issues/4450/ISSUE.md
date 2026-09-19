# D2-01: D2-01: FO3/FNV CritChance/MeleeDamage/UnarmedDamage ship an uncited ActorGeneral scope that fallout.rs asserts as fact and GetActorValue consumes live

- **Labels**: medium,character,bug,game:fo3,game:fnv
- **Filed from**: docs/audits/AUDIT_CHARACTER_2026-09-19.md (/audit-character 2026-09-19)
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/4450

---

**Source**: NONE — the finding is the unsourcedness itself. Nearest lines: charal-fnv-fo3-ruleset.md:91-101 (the derived table annotates scope on Health, Carry Weight, Rad/Poison, and AP — and gives NO scope for Critical Chance / Melee Damage / Unarmed Damage); :388-397 (#2937: the project explicitly rejects deriving scope without per-stat NPC evidence).

**Description**

When #2937 faced exactly this question for Action Points — formula locked, scope unstated by any capture — the code chose `player_only` conservatively, documented the choice at the call site, pinned it with a test, and left an open item. The three sibling rows in the same table received no such treatment: they default to `DerivedScope::ActorGeneral`, and the module docstring upgrades the default to a factual claim ("The six derived stats shared verbatim by FO3 and FNV (all actor-general)", fallout.rs:44).

**Evidence**

`add_fnv_fo3_shared` (fallout.rs:45-85) tags none of the CritChance / MeleeDamage / UnarmedDamage rows with `.player_only()` and records no #2937-style rationale. `GetActorValue` (crates/scripting/src/condition.rs:500-531) gates on `scope == ActorGeneral`, so an FO3/FNV NPC queried for these AVs gets a formula-computed value today; `melee_damage_charal_bonus` (byroredux/src/combat.rs:432-460) applies MeleeDamage live.

**Impact**

If any of the three is genuinely player-only in the engine, NPCs silently receive computed Critical Chance / Melee Damage values they should not — the exact over-computation risk #2937 chose to avoid. Plausibly correct (the GECK auto-calc populates these for NPCs too), but "plausible" is what the no-guessing doctrine exists to prevent.

**Related**

#2937 (the mirror-image decision for AP); #2936 (percentage-convention discipline in the same rows, fixed).

**Suggested Fix**

Mirror #2937's treatment: find a per-row citation (fandom Critical Chance / Melee Damage / Unarmed Damage NPC behavior) and record it in charal-fnv-fo3-ruleset.md's derived table, or annotate the three rows "scope unsourced" in the doc and note the explicit choice in `fallout.rs` with a pinning test, so a future flip is a reviewed edit.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other per-game tables, other doc sites making the same claim)
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed from `docs/audits/AUDIT_CHARACTER_2026-09-19.md` (finding D2-01, /audit-character 2026-09-19, HEAD `479163836`).*