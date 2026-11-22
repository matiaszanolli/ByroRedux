# #7216 CHAR-2026-11-21-L-19: `charal.md` §4.7 and §5 still describe the pre-#7010 Oblivion (builder unwired, regen lanes deleted, `PoolRegenConfig` shape, "unread" rows)

**Labels**: low,character,documentation,doc-rot,game:oblivion

From `docs/audits/AUDIT_CHARACTER_2026-11-21.md` (`/audit-character`, HEAD `a06d5f9db`).

- **Reported as**: CHAR-D5-02
- **Severity**: LOW
- **Dimension**: Coverage & Doctrine
- **Game**: oblivion
- **Location**: `docs/engine/charal.md` §4.7 ("Mechanism BUILT and registered; live for FO3/FNV Action Points only", the Oblivion paragraph citing `oblivion_pool_regen_config` and "blocked on Oblivion's own wiring, #3768 (#4460)", "Stunted Magicka ... always passed `false`", and the `PoolRegenConfig` struct sketch); §5 (the paragraph "**The Oblivion ruleset builder is complete — it is unwired.** `CharacterRulesProfile::OBLIVION` carries `ruleset: RulesetBuilder::None` ... `Oblivion.esm` authors no `AVIF` records ... a legacy actor-value index resolver is needed" and "Oblivion's `LightArmorRating` / `HeavyArmorRating` rows are equally unread: Oblivion has no runtime ruleset yet")
- **Status**: NEW (the `charal.md:263` spelling is Dim 3's; Dim 3's regen item covers `charal-oblivion-ruleset.md`, not this file's prose)
- **Description**:
  - §4.7 says the Oblivion Fatigue/Magicka lanes need a deleted builder and that re-adding it "is blocked on Oblivion's own wiring". The lanes are now installed by `CharacterRulesProfile::build_tes_pool_regen` (`NpcStatModel::OblivionNpc` guard; fills `fatigue_avif`, `magicka_avif`, `willpower_avif`, `stunted_magicka_avif`) at `cell_loader/references/mod.rs`. The tick passes `stunted` from `stunted_magicka_avif` (not a constant `false`), and the struct sketch omits the `stunted_magicka_avif` field the real `PoolRegenConfig` has. The heading's "live for FO3/FNV Action Points only" is also false.
  - §5 says Oblivion's profile is `RulesetBuilder::None`. It is `RulesetBuilder::Oblivion`. It says no AVIF resolver exists. The engine supplies the 72 Oblivion records (ROADMAP: index coverage 70 / 70, `oblivion_actor_values.rs`), and `docs/feature-matrix.md` is cited as backing "~ built, unwired" (which is itself stale: CHAR-D5-01).
  - §5's pool formulas "Health = 2×Endurance, Magicka = 2×Intelligence" are the `tes.rs` builder rows, while the player's pools are formulas of `fPCBaseHealthMult` / `fPCBaseMagickaMult` (#7037/#7041; see the `RulesetBuilder::Oblivion` arm comment in `build_ruleset`). The section does not mention that split.
- **Evidence**: `profile.rs` `OBLIVION` const and `build_tes_pool_regen`; `regen.rs` `PoolRegenConfig` fields; `oblivion_builds_its_runtime_ruleset`.
- **Impact**: `charal.md` is the layer's design doc and `/audit-character` reads it first; it tells the reader the Oblivion ruleset is dead and regen is inert.
- **Related**: #3768 (CLOSED 2026-08-31, cited as an open blocker), #4455 (CLOSED, same paragraph), #4460 (CLOSED), CHAR-D3-05, CHAR-D5-01.
- **Suggested Fix**: Rewrite the §5 "complete — unwired" paragraph to "wired (#7010, `RulesetBuilder::Oblivion`)" with the engine-supplied roster as the AVIF source; rewrite the §4.7 Oblivion paragraph and the struct sketch (add `stunted_magicka_avif`, name `build_tes_pool_regen`); state the player-formula versus builder-row split; drop the #3768 / #4460 blocker wording.

- **Merged**: also fix `charal.md:350-351` ("Oblivion's `LightArmorRating` / `HeavyArmorRating` rows are equally unread: Oblivion has no runtime ruleset yet", from CHAR-D2-02; the rows are unread because the roster has no such value, L-04), `charal.md:367` ("the Oblivion builder is unwired", from CHAR-D1-02), and the `fFatigureReturnMult` spelling at `charal.md:263` (the master's setting is `fFatigueReturnMult`, from CHAR-D3-02 / M-05).

## Completeness Checks
- [ ] **SIBLING**: Same stale claim grepped across `docs/engine/charal*.md`, `docs/feature-matrix.md`, `ROADMAP.md` and code comments
- [ ] **TESTS**: Where a source-text pin is suggested, it is added in the same commit
