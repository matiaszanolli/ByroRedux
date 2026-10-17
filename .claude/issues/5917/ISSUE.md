# CHAR-2026-10-16-M-04: Level-up skill points read the player's *current* Intelligence; the capture says base INT only

Labels: medium,character,bug,game:fo3,game:fnv
Filed from: docs/audits/AUDIT_CHARACTER_2026-10-16.md

- **Severity**: MEDIUM
- **Dimension**: Progression & Pools / Population Boundary
- **Game**: fo3, fnv
- **Location**: `byroredux/src/combat.rs:1128-1143` (`player_intelligence`), used at `:655` (`grant_experience`) and `:1166` (`scripted_experience_system`)
- **Status**: NEW
- **Source**: `charal-fnv-fo3-ruleset.md:78-79`: "Temporary INT (apparel/chems) does **not** change Skill Rate — only the base INT at level-up." (Skill Rate rows `:72-73`: FO3 `10 + INT`, FNV `10 + INT/2`.)
- **Description**: `player_intelligence` evaluates `GetActorValue`, which is `ActorValueView::Current` (`base + set_override + permanent + temporary − damage`). A +INT chem or apparel effect, or a damage layer, at the instant of the roll changes the banked points. `ActorValueView::Base` exists (since `44e4d948d`) but is not used. `permanent_mod` also carries equipped gear, so no existing view equals "base INT incl. Intense Training but excl. gear/chems": the fix needs a deliberate choice.
- **Evidence**: `player_intelligence` → `evaluate_function(GetActorValue, Intelligence)`; `condition/eval.rs:458` `GetActorValue => actor_value_view(.., Current)`.
- **Impact**: Wrong skill points whenever INT is temporarily modified (exploitable: take an INT chem, then kill). The FO4 XP multiplier shares the helper but its capture (`charal-fo4-ruleset.md:44,497`) does not say base-vs-current, so only the Fallout 3/NV use is claimed.
- **Suggested Fix**: Read INT through the Base view plus the non-gear permanent layer, or `undamaged() − temporary_mod`; pin with a test that applies `mod_temporary(INT, +3)` and asserts the banked points are unchanged.

---
Filed from `docs/audits/AUDIT_CHARACTER_2026-10-16.md` (HEAD fa7591ca2).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other games' rulesets, sibling writers/consumers)
- [ ] **TESTS**: A regression test pins this specific fix
