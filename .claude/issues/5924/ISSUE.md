# CHAR-2026-10-16-L-03: `levelup` spends points in the wrong layer and skips the rank gate

Labels: low,character,bug,gameplay,game:fo3,game:fnv
Filed from: docs/audits/AUDIT_CHARACTER_2026-10-16.md

- **Severity**: LOW (console stopgap until the level-up menu exists)
- **Dimension**: Progression & Pools / Population Boundary (layer contract)
- **Game**: fo3, fnv
- **Location**: `byroredux/src/commands/gameplay.rs` `LevelUpCommand` (`values.mod_permanent(avif, spend)`, `:404-415`; `owned.set_rank(perk, rank)`, `:437-460`); test `:598-600` pins rank 2
- **Status**: NEW
- **Source**: `charal-fnv-fo3-ruleset.md:60-72` (player skill is "base + tag + Skill Rate points", so allocated points are a base component); `actor_values.rs` (`permanent_mod` is perks/enchantments/gear and the magic runtime owns it as the constant-spell sum); `Perks::try_set_rank` doc ("a level-up path granting a rank the PERK record never defined … is a bug at the call site").
- **Description**: Allocated skill points land in `permanent_mod`, which `ActorValueView::Base` peels off (`condition/eval.rs:411`), so `GetBaseValue` under-reports spent points (Skyrim's own `advance_skill` correctly uses `set_base`). `levelup perk` computes `rank = owned.rank(perk) + 1` and calls raw `set_rank`: no `PerkRecord::num_ranks` bound, no check the id is a PERK, and `try_set_rank` has zero production callers. The unit test grants Educated rank 2 from a `PerkRecord { ..Default::default() }` and asserts success, enshrining the bypass.
- **Impact**: `GetBaseValue`/`GetBaseActorValue` reads are low after spending; repeated `levelup perk` can push a perk past its rank count, and `levelup perk DEADBEEF` creates a ghost entry that feeds `compose_entry_point` and is saved.
- **Suggested Fix**: Write the spend with `set_base(base + spend)`; resolve `index.perks[perk].num_ranks` and use `try_set_rank`; fix the test to use a record with explicit `num_ranks`.

---
Filed from `docs/audits/AUDIT_CHARACTER_2026-10-16.md` (HEAD fa7591ca2).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other games' rulesets, sibling writers/consumers)
- [ ] **TESTS**: A regression test pins this specific fix
