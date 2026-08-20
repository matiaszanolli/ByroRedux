# CHAR-2026-08-20-D2-01: SkillSet::SKYRIM spells Illusion "Illusion"; vanilla Skyrim.esm authors it as AVMysticism (0x45B) — same defect class as the just-fixed FNV Guns/Survival

**Issue**: #3169 — https://github.com/matiaszanolli/ByroRedux/issues/3169
**Finding ID**: `CHAR-2026-08-20-D2-01`
**Severity**: MEDIUM
**Dimension**: 2 — Derived Formulas (roster identity)
**Audit**: `/audit-character` — 2026-08-20 comprehensive suite, HEAD `bb0b92f2`
**Labels**: medium, legacy-compat, bug

---

**Audit**: `/audit-character` — `docs/audits/AUDIT_CHARACTER_2026-08-20.md` (HEAD `bb0b92f2`)
**Finding ID**: `CHAR-2026-08-20-D2-01`
**Severity**: MEDIUM
**Dimension**: 2 — Derived Formulas (roster identity)
**Game**: Skyrim SE

## Location

`crates/core/src/character/skill.rs:118-147` — the `SkillSet::SKYRIM` roster, specifically the
`SkillDef::ungoverned("Illusion")` entry at `:135`, and the docstring at `:118-126` that
enumerates the known EditorID/display-name divergences.

## Description

Skyrim retained legacy `AVIF` record identities under new display names in at least **three**
places. The roster docstring already documents two of them — *"Archery = `Marksman`, Speech =
`Speechcraft`"* — and gets both right. It missed the third: **Illusion reuses the `AVMysticism`
record.**

Because `actor_value_form_id` normalizes only the `AV` **prefix** (#2986), `resolve("Illusion")`
tries `Illusion` then `AVIllusion`, and **both miss**. 17 of 18 Skyrim skills resolve; Illusion
does not.

## This is the same defect class as the just-fixed FNV `Guns` / `Survival`

`CHAR-2026-08-16-D2-02` (**#3094**, CLOSED) was exactly this: a Bethesda **display-name rename
that left the record identity alone**, written into the roster as the display name. That fix split
`SkillSet::FALLOUT_FO3_FNV` into `FALLOUT3` / `FALLOUT_NV` and corrected FNV's `Guns` →
`SmallGuns` and `Survival` → `Throwing`.

**This key survived the very commit that fixed FNV's**, one game over. It is structurally
identical, and it is the third such retention in the Skyrim roster alongside the two the docstring
already documents.

## Source / Evidence (vanilla `Skyrim.esm`, independent binary parser)

The 18 Skyrim skills occupy a contiguous FormID block `0x0000044C..0x0000045D`:

```
0000044C AVOneHanded    00000450 AVSmithing     00000454 AVLockpicking  00000458 AVAlteration
0000044D AVTwoHanded    00000451 AVHeavyArmor   00000455 AVSneak        00000459 AVConjuration
0000044E AVMarksman     00000452 AVLightArmor   00000456 AVAlchemy      0000045A AVDestruction
0000044F AVBlock        00000453 AVPickpocket   00000457 AVSpeechcraft  0000045B AVMysticism
                                                                        0000045C AVRestoration
                                                                        0000045D AVEnchanting
```

Exactly 18 records for exactly 18 skills. The only EditorID in that block that is **not** a Skyrim
skill name is `AVMysticism` — Oblivion's retired school — occupying the slot between Destruction
and Restoration **where Illusion belongs**.

A search of the whole master for any `AVIF` whose EditorID contains `Illusion` returns only
`AVIllusionMod` (`0x616`), `AVIllusionSkillAdvance` (`0x628`) and `AVIllusionPowerMod` (`0x63D`)
— the three *modifier* actor values, never the skill itself.

Cross-checked and **not** wrong: `Marksman` (`0x44E`) and `Speechcraft` (`0x457`) both resolve, so
the docstring's existing claims are accurate. Only Illusion is wrong.

## Impact

Latent today and bounded — `SkillSet::SKYRIM` has **no production reader**
(`CharacterRulesProfile::SKYRIM` uses `NpcStatModel::RaceBaseOffsets`, `profile.skills()` is
consumed only on the `ClassAutoCalc` branch, and `skyrim_ruleset` has no construction site). That
is why this is MEDIUM and not HIGH.

It becomes live the moment Skyrim gets a `RulesetBuilder` arm (see `CHAR-2026-08-20-D3-01`, whose
cheapest fix is exactly that) or a skill-XP progression runtime — at which point Skyrim's
`CharacterRuleset` carries **17 of 18 skills**, and every Illusion-gated condition, perk
requirement and skill-XP feed silently reads the absent-AV default `0.0`. Filing it now is cheaper
than debugging one missing magic school later.

## Related

- **#3094** — CLOSED; the same defect class on FNV, fixed in the commit this survived.
- `CHAR-2026-08-20-D6-01` — the test that would have caught this covers FNV only. **That
  test-coverage gap is the durable half of this finding.**
- **#2986** / `ESM-2026-08-16-D7-01` — the `AV` prefix normalization, which this defect survives
  because the divergence is in the *stem*, not the prefix.

## Suggested Fix

1. Change the entry to `SkillDef::ungoverned("Mysticism")`.
2. Extend the roster docstring's rename list from two entries to three:
   `Archery = Marksman`, `Speech = Speechcraft`, **`Illusion = Mysticism`**.
3. Extend the `#[ignore]`d real-data test (`crates/plugin/tests/parse_real_esm.rs:177-193`) to
   loop `SkillSet::SKYRIM` the way its FNV sibling loops `SkillSet::FALLOUT_NV`, so the fix is
   pinned **by data** rather than by another hand-written string.

## Completeness Checks
- [ ] **SIBLING**: the other four rosters (`FALLOUT3`, `OBLIVION`, `AttributeSet::*`) are checked against their shipped masters for the same display-name-vs-record-identity divergence
- [ ] **CANONICAL-BOUNDARY**: the fix stays a roster-identity correction at the CHARAL rules seam — no per-game branch is added downstream of `CharacterRuleset`
- [ ] **TESTS**: `parse_real_esm.rs` loops `SkillSet::SKYRIM` against `Skyrim.esm` and would fail on the `"Illusion"` spelling
