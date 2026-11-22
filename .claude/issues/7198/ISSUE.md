# #7198 CHAR-2026-11-21-L-01: The Oblivion player's Magicka formula is a literal `2 x Intelligence`: neither `fPCBaseMagickaMult` nor the Magicka Multiplier actor value (40) is read

**Labels**: low,character,bug,game:oblivion

From `docs/audits/AUDIT_CHARACTER_2026-11-21.md` (`/audit-character`, HEAD `a06d5f9db`).

- **Reported as**: CHAR-D4-02
- **Severity**: LOW
- **Dimension**: Population Boundary
- **Game**: Oblivion
- **Location**: `crates/core/src/character/tes.rs:54-56` (`oblivion_magicka_formula`), `crates/core/src/character/profile.rs:889` (`RulesetBuilder::Oblivion => oblivion_ruleset(resolve)`, no GMST argument unlike the three Fallout arms)
- **Status**: NEW
- **Source**: `docs/engine/charal-oblivion-ruleset.md:1175` (player Magicka = `trunc(Int x (1 + fPCBaseMagickaMult))`, GMST 1.0, then x AV 40 / 10; the zero-multiplier rewrite to 1.0 at `0x5ead3d` is stated in the same section) and `:723` (UESP: `Int + Int x fPCBaseMagickaMult (1.0)`). `Oblivion.esm` authors `fPCBaseMagickaMult` = 1.0 (byte scan); `fPCBaseHealthMult` is not authored (exe default 2.0), so Health's literal 2.0 is correct.
- **Description**: the NPC auto-calc path reads `fNPCBaseMagickaMult` from the load order (`oblivion_population.rs` `setting(...)`), but the player's row restates the coefficient as a literal 2.0 and takes no setting, which is exactly the "coefficient restated beside its GMST" regression that #6756 retired for the Fallout rows. Separately the capture's multiplier term (AV 40, 0 meaning 1.0) has no consumer: `MagickaMultiplier` is installed in the roster (`oblivion_actor_values.rs:51`) but nothing reads it, and no known-gap line in "Known gaps against the executable" or "The player's pools" declares that.
- **Impact**: a mod retuning `fPCBaseMagickaMult` is ignored for the player; an effect that writes the Magicka Multiplier value changes nothing. Vanilla values (GMST 1.0, multiplier unset) are unaffected. How often vanilla content writes AV 40 was not measured.
- **Related**: #6756 (Fallout precedent), #7037/#7041 (player pools).
- **Suggested Fix**: add `oblivion_ruleset_with_gmst` reading `fPCBaseMagickaMult` (slope `1 + mult`); either model the AV-40 multiplier as a `Multiplier`-style row or add it to the declared gaps with the capture line.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other CHARAL rule rows / overlays / profiles)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
