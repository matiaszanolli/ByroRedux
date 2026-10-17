# CHAR-2026-10-16-L-06: No actor-value writer exists for NPC Morality, so `GetBribeAmount` prices every NPC as Morality 0

Labels: low,character,bug,game:skyrim
Filed from: docs/audits/AUDIT_CHARACTER_2026-10-16.md

- **Severity**: LOW
- **Dimension**: Derived Formulas / Population
- **Game**: skyrim
- **Location**: `crates/scripting/src/bribe.rs` (`bribe_amount`/`bribe_inputs` read `rules.morality` through `actor_value_view`); `ActorAiData` (`crates/plugin/src/esm/records/actor/mod.rs:39`) carries only aggression, confidence and attack radius
- **Status**: NEW
- **Source**: `charal-skyrim-ruleset.md:78-81` `NpcMorality = NpcMoralityAV × fBribeMoralityMult + 1.0`; domain 0 Any Crime / 1 Violence Against Enemies / 2 Property Crime Only / 3 No Crime (`uesp-wiki/Skyrim/Followers.wiki:42-49`). **The AIDT byte that carries it is in no charal capture**, so the population mapping is unsourced and must be captured before fixing.
- **Impact**: Exact for Any-Crime NPCs, low for the rest (a "No Crime" NPC is under-quoted up to 4×). Bounded to one dialogue option.
- **Related**: #4414 (AIDT decode, closed).
- **Suggested Fix**: Capture the AIDT Responsibility byte (xEdit `wbAIDT`) in a charal-skyrim note, decode it into `ActorAiData`, stamp it as the `Morality` AV at spawn.

---
Filed from `docs/audits/AUDIT_CHARACTER_2026-10-16.md` (HEAD fa7591ca2).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other games' rulesets, sibling writers/consumers)
- [ ] **TESTS**: A regression test pins this specific fix
