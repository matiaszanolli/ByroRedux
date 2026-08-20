# CHAR-2026-08-20-D2-02: the two GMST names cited for the skill auto-calc coefficients are not authored by any shipped Fallout master, and the 13 that are authored are shadowed by one shared constant

**Issue**: #3173 — https://github.com/matiaszanolli/ByroRedux/issues/3173
**Finding ID**: `CHAR-2026-08-20-D2-02`
**Severity**: LOW
**Dimension**: 2 — Derived Formulas
**Audit**: `/audit-character` — 2026-08-20 comprehensive suite, HEAD `bb0b92f2`
**Labels**: low, legacy-compat, documentation

---

**Audit**: `/audit-character` — `docs/audits/AUDIT_CHARACTER_2026-08-20.md` (HEAD `bb0b92f2`)
**Finding ID**: `CHAR-2026-08-20-D2-02`
**Severity**: LOW
**Dimension**: 2 — Derived Formulas
**Game**: FO3, FNV

## Location

`crates/plugin/src/esm/records/actor_value_derive.rs:81-97` — the `SKILL_BASE` /
`SKILL_ATTR_MULT` / `SKILL_LUCK_MULT` block and its `#2934 — DOCTRINE NOTE`; echoed in the module
docstring at `:23-28` and in `crates/core/src/character/ruleset.rs:120-133`.

## Description

The code annotates each coefficient with a GMST name:

```rust
const SKILL_BASE: f32 = 2.0;       // fAVDSkill<name>Base
const SKILL_ATTR_MULT: f32 = 2.0;  // fAVDSkillPrimaryBonusMult
const SKILL_LUCK_MULT: f32 = 0.5;  // fAVDSkillLuckBonusMult
```

and #2934's doctrine note defers moving them onto `CharacterRuleset` because that move is
*"deliberately paired with sourcing them from GMSTs (#2942)"*. Two problems at HEAD:

**1. `fAVDSkillPrimaryBonusMult` and `fAVDSkillLuckBonusMult` do not exist.** A raw byte search of
both `FalloutNV.esm` and `Fallout3.esm` finds neither string. (`fAVDTagSkillBonus` **is** present,
so the search is sound and the family name is right.) There is nothing to source those two from —
**the planned route is a dead end as written**, and #2942 being closed makes the precondition read
as satisfied.

**2. `fAVDSkill<Name>Base` is per-skill, not shared.** FNV authors **thirteen** of them —
`fAVDSkillBarterBase`, `fAVDSkillBigGunsBase`, `fAVDSkillEnergyWeaponsBase`,
`fAVDSkillExplosivesBase`, `fAVDSkillLockpickBase`, `fAVDSkillMedicineBase`,
`fAVDSkillMeleeWeaponsBase`, `fAVDSkillRepairBase`, `fAVDSkillScienceBase`,
`fAVDSkillSmallGunsBase`, `fAVDSkillSneakBase`, `fAVDSkillSpeechBase`, `fAVDSkillSurvivalBase` —
all `2.0` in vanilla, and all collapsed into **one** engine constant.

⚠️ Note the trap: **the GMST family keys on the *display* name** (`…SurvivalBase`, not
`…ThrowingBase`) — the **inverse** of the `AVIF` convention, where the record identity is
`AVThrowing`. Whoever wires this will hit that.

## Evidence

`/tmp/audit/character/gmst.py` over both masters with pattern
`AVDSkill|XPLevel|fAVDActionPoints|AVDHealth|fAVDCarry`; plus targeted raw `bytes-in-file` checks
for the two absent names (both `absent` on FNV; the FO3 GMST scan with
`AVDSkillPrimary|AVDSkillLuck` returns nothing either).

Doc source for the cited values: `docs/engine/charal-fnv-fo3-ruleset.md:47` cites geckwiki
*Derived Skill Settings* for `fAVDSkillBase=2`, `…PrimaryBonusMult=2` — so the **values** are
sourced and correct; it is the **GMST names as authored by the masters** that the comments get
wrong.

## Impact

**No wrong number today** — every vanilla value is `2.0` / `2.0` / `0.5`, which is what the code
uses. The cost is directional:

- The recorded plan for closing #2934's doctrine gap **points at two GMSTs that are not there**.
- The thirteen that *are* there are invisible to the reader of that comment.
- A mod retuning one skill's base is silently ignored.

## Related

- **#2934** — CLOSED; the doctrine note that cites these names.
- **#2942** — CLOSED; the GMST-sourcing precondition that now reads as satisfied.
- `CHAR-2026-08-20-D3-01` — the same GMST-reach problem one layer up (`with_gmst` has zero
  production reach).

## Suggested Fix

Correct the comments to what the masters actually author: per-skill
`fAVDSkill<DisplayName>Base`, and mark the two mult names as **geckwiki-documented but unauthored**
(engine defaults) rather than as a pending GMST read.

When `skill_calc` finally lands on `CharacterRuleset`, source the per-skill base **by display
name** with `2.0` as the fallback, and leave the two mults as engine constants with that fact
recorded.

Do this in the same commit as `CHAR-2026-08-20-D3-01` so the recorded plan and the code agree.

## Completeness Checks
- [ ] **SIBLING**: the echoed copies at `actor_value_derive.rs:23-28` and `crates/core/src/character/ruleset.rs:120-133` are corrected too, not just the const block
- [ ] **CANONICAL-BOUNDARY**: when sourced, the per-skill GMST read happens once at the CHARAL ruleset seam, keyed by display name — not re-derived per consumer
