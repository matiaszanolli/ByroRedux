# CHAR-2026-08-20-D6-01: the #3095 real-data existence test covers one roster out of five and no derived-row output key on any game

**Issue**: #3172 — https://github.com/matiaszanolli/ByroRedux/issues/3172
**Finding ID**: `CHAR-2026-08-20-D6-01`
**Severity**: MEDIUM
**Dimension**: 6 — Coverage, Documentation & Doctrine Drift
**Audit**: `/audit-character` — 2026-08-20 comprehensive suite, HEAD `bb0b92f2`
**Labels**: medium, legacy-compat, bug

---

**Audit**: `/audit-character` — `docs/audits/AUDIT_CHARACTER_2026-08-20.md` (HEAD `bb0b92f2`)
**Finding ID**: `CHAR-2026-08-20-D6-01`
**Severity**: MEDIUM
**Dimension**: 6 — Coverage, Documentation & Doctrine Drift
**Game**: all

## Location

- `crates/plugin/tests/parse_real_esm.rs:126-176` — `fnv_actor_value_roster_and_health_resolve_on_shipped_master`
- `crates/plugin/tests/parse_real_esm.rs:177-193` — `skyrim_health_resolves_to_authored_avif_form_id`
- against `crates/core/src/character/fallout.rs:229-247` (`full` / `fo4_full`),
  `crates/core/src/character/skyrim.rs:168-176`, `crates/core/src/character/tes.rs`

## Description

**#3095** recorded that every CHARAL builder test hands the builder a resolver written from the
roster's own strings, so **no test can falsify a roster**. The fix added two `#[ignore]`d
real-data tests. One of them genuinely closes the gap — **for FNV**:

```rust
for skill in SkillSet::FALLOUT_NV.skills() {
    assert!(index.actor_value_form_id(skill.editor_id).is_some(), ...);
}
```

That loop is falsifiable: it would have failed on the old `"Guns"` / `"Survival"` spellings.

**Nothing equivalent exists for `SkillSet::SKYRIM`, `SkillSet::FALLOUT3`, `SkillSet::OBLIVION`, or
`AttributeSet::*`** — and **no test on any game** asserts that a builder's *derived-row output
keys* (`CarryWeight`, `MeleeDamage`, `CritChance`, `UnarmedDamage`, `RadResist`, `PoisonResist`,
`DamageResist`, `LightArmor`, `Stamina`, `ActionPoints`) resolve against a shipped master. Those
still rely exclusively on the hand-written `full()` fixture, which enumerates the same strings the
builders pass.

The Skyrim half of the fix asserts a single FormID (`health_actor_value_key() == Some(0x3E8)`) and
one non-zero derived Health — worth having, but it does not touch a roster.

## The coverage arithmetic: one roster of five, no derived-row key on any game

| Roster | Falsifiable against real data? |
|---|---|
| `SkillSet::FALLOUT_NV` | ✅ yes (this is the #3095 fix) |
| `SkillSet::FALLOUT3` | ❌ no |
| `SkillSet::SKYRIM` | ❌ no |
| `SkillSet::OBLIVION` | ❌ no |
| `AttributeSet::*` | ❌ no |
| **derived-row output keys, every game** | ❌ **no test on any game** |

**`CHAR-2026-08-20-D2-01` is the demonstration**: an unresolvable roster key (`"Illusion"`, where
`Skyrim.esm` authors `AVMysticism`) survived the very commit that closed #3095 — in a roster the
new test does not loop. **That is exactly how the key survived, and this test-coverage gap is the
durable half of that finding.**

## Evidence

`grep -n "SkillSet::" crates/plugin/tests/parse_real_esm.rs` → only `FALLOUT_NV` (`:145`, `:153`).

`grep -rn "fn full(" crates/core/src/character/` still returns the synthetic resolvers in
`fallout.rs`, `skyrim.rs` and `tes.rs`, unchanged in shape.

The FO3, FNV and FO4 derived keys *were* verified by hand this session and **do** all resolve
(`AVCarryWeight 0x44D`, `AVMeleeDamage 0x451`, `AVCritChance 0x44E`, `AVUnarmedDamage 0x5E6`,
`AVRadResist 0x454`, `AVPoisonResist 0x453` — present on both FO3 and FNV masters) — but **that
verification lives in an audit report, not in the suite.**

## Impact

Process, not runtime. The suite can now falsify **one roster out of five**; the other four, and
every derived-row key on every game, remain in the pre-#3095 state where a key that does not exist
on disk produces a **green test and an empty table**.

Given that three of the last sweep's four findings were instances of exactly this class, the
residual exposure is the main reason to finish the job rather than call it closed.

## Related

- **#3095** — CLOSED, half-done. This is the remainder.
- `CHAR-2026-08-20-D2-01` — what slipped through the gap.
- **#2986** / `ESM-2026-08-16-D7-01` — the `AV`-prefix normalization that these tests sit on top of.
- Folded in from a disproved candidate: `character_rules_profile`'s FO3-vs-FNV `HEDR < 1.0` split
  is pinned on the FNV side by real data (`parse_real_esm.rs:140-143`) but only by a synthetic-HEDR
  unit test on the FO3 side — weaker, not wrong, and the generalized helper below closes it too.

## Suggested Fix

Generalize the FNV loop into **one helper** taking `(master path, CharacterRulesProfile)` and
assert, per implemented family:

1. every `AttributeSet` member resolves,
2. every `SkillSet` member resolves,
3. `build_ruleset` against the **real** index produces the expected `derived_row_len()`.

Run it over `Fallout3.esm`, `FalloutNV.esm`, `Fallout4.esm` and `Skyrim.esm`. **Existence is the
whole assertion — no values needed.** Keep the synthetic fixtures for the arithmetic, where they
are the right tool.

## Completeness Checks
- [ ] **SIBLING**: all five rosters are looped, not only the two games with the loudest failures
- [ ] **SIBLING**: derived-row *output* keys are asserted too, not just input rosters — that half has zero coverage on every game today
- [ ] **CANONICAL-BOUNDARY**: the helper asserts against the real `EsmIndex` resolver at the parser boundary, never against a roster-derived fixture
- [ ] **TESTS**: the generalized helper fails on the current `"Illusion"` spelling before `CHAR-2026-08-20-D2-01` is fixed
