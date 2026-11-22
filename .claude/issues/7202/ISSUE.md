# #7202 CHAR-2026-11-21-L-05: The Oblivion `ROSTER_CASES` entry asserts nothing about the installed roster or derived outputs, and its neighbouring prose is stale

**Labels**: low,character,test-gap,bug,game:oblivion

From `docs/audits/AUDIT_CHARACTER_2026-11-21.md` (`/audit-character`, HEAD `a06d5f9db`).

- **Reported as**: CHAR-D1-02
- **Severity**: LOW
- **Dimension**: Ruleset Seam
- **Game**: Oblivion
- **Location**: `crates/plugin/tests/parse_real_esm.rs:938-948` (`RosterCase` Oblivion, `derived_rows: None`, `authors_actor_values: false`) and `:1008-1020` (early `return`); `crates/plugin/src/esm/records/tests.rs:166-193`; `docs/engine/charal.md:367`; `crates/core/src/character/tes.rs:109-112`
- **Status**: NEW
- **Description**: `assert_rosters_resolve` returns at the `!authors_actor_values` branch (it only checks that the *parsed* master has no AVIF) before it reaches the roster, `skills()` and `derived_rows` assertions, and it never installs the engine roster (`parse_esm` leaves `actor_values` empty; the loader installs afterward). So `AttributeSet::TES_CLASSIC`, `SkillSet::OBLIVION` and the `oblivion_ruleset` output keys are not falsified against the roster the engine actually uses, in an ignored test or in a unit test (`derived_row_len` is asserted for no Oblivion build). That is how CHAR-D1-01 escaped. The `derived_rows: None` arm's own message ("if one landed, give this case its expected derived_rows") was written for a builder that now exists (`RulesetBuilder::Oblivion`). Separately, `records/tests.rs:166-179` still says Oblivion "carries `RulesetBuilder::None`, confirmed unreachable in production", `charal.md:367` says the Oblivion builder "is unwired" and needs a legacy resolver, and `tes.rs:111` says "Pre-AVIF Oblivion supplies a resolver ... at the parser boundary": all false since #7008-#7010.
- **Evidence**: `oblivion_ruleset_resolves_nothing_against_avif_pending_a_legacy_resolver` (`records/tests.rs:180`) still passes only because it never installs the roster. `oblivion_builds_its_runtime_ruleset` (`profile.rs:1356`) is `build_ruleset(|_| None, ...).is_some()`.
- **Impact**: a misspelled or absent Oblivion roster/derived name (the exact failure of CHAR-D1-01) stays green; three docs mislead a reader about whether Oblivion has a runtime ruleset.
- **Related**: CHAR-D1-01; #4094 (the FO76/Starfield `ROSTER_CASES` precedent); Dimension 5 owns the doc sweep.
- **Suggested Fix**: install the engine roster in the Oblivion case (as `oblivion_population_census_stamps_every_standard_value` does) and assert `attributes`, `skills` and the built `derived_row_len()` with the GMST overlay; add a non-ignored unit test doing the same on a synthetic installed index; rewrite the three stale statements.

- **Merged**: the three stale statements named here (`records/tests.rs:166-179`, `charal.md:367`, `tes.rs:109-112`) are fixed under L-19 and L-21; this finding is the vacuous test.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other CHARAL rule rows / overlays / profiles)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
