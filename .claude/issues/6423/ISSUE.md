# CHAR-2026-10-26-L-01: `build_vats_model` is a raw `GameKind` gate plus a whole-profile-equality FO3-vs-FNV discriminator; the VATS dialect is not a profile row

- **Severity**: LOW (Dim 1 rated MEDIUM. Behaviour is correct for both shipped profiles, so this is rated on impact.)
- **Dimension**: Ruleset Seam
- **Game**: fo3, fnv (matters for FO4 when its VATS lane lands)
- **Location**: `byroredux/src/npc_spawn.rs:322-335`; doc `profile.rs:196-210`
- **Status**: NEW
- **Source**: doctrine #5921 `3162b21b8` / #4447 / #4679, and `profile.rs:198-210` itself ("Prefer adding a policy row of the consumer's own before adding a third such consumer"). Not numeric.
- **Description**: `if index.game != GameKind::Fallout3NV { return None }` decides whether the game has VATS at all. Comparing the entire `CharacterRulesProfile` against `FALLOUT_NEW_VEGAS` decides which model applies. That is a third profile-identity borrower, and the profile's documented exceptions list only two. The FO3 leg (`FALLOUT3`, 4/s) is exercised by no test. A new family must edit the consumer, and the `per_game_rule_rows_are_stated_on_every_profile` pin cannot see it. The crime consumers were checked and do not have this problem.
- **Related**: #5921, #6350, L-02, L-04.
- **Suggested Fix**: Add `vats: Option<VatsModel>` to `CharacterRulesProfile` (`FALLOUT3`/`FALLOUT_NV` rows, `None` elsewhere). `build_vats_model` becomes `index.character_rules.vats()?.with_gmst(..)`. Extend the row pin, and add an FO3-profile leg asserting 4.0/s.

---
Source: `docs/audits/AUDIT_CHARACTER_2026-10-26.md` (HEAD bf304851f), finding `CHAR-2026-10-26-L-01`. Fix with: `/fix-issue <this issue>`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / profiles / crates
- [ ] **TESTS**: A regression test or source-scan guard pins this specific fix (or the finding is doc-only and `_audit-validate.sh` / `cargo test` stays green)

