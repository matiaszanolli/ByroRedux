# #7203 CHAR-2026-11-21-L-06: The new Oblivion/Skyrim rule installers have no direct test, and none proves the profile wins over the game

**Labels**: low,character,test-gap,bug,game:oblivion,game:skyrim

From `docs/audits/AUDIT_CHARACTER_2026-11-21.md` (`/audit-character`, HEAD `a06d5f9db`).

- **Reported as**: CHAR-D1-03
- **Severity**: LOW
- **Dimension**: Ruleset Seam
- **Game**: Oblivion, Skyrim
- **Location**: `byroredux/src/player_cast.rs:23` (`install_spell_cast_model`), `byroredux/src/combat.rs:560,612,623` (`install_armor_rating_model`, `install_fall_model`, `install_block_model`); `byroredux/src/crime.rs:995-1030`; `byroredux/src/systems/faction_hostility.rs:82`; `crates/core/src/character/profile.rs` tests
- **Status**: NEW
- **Description**: #6760's rule (a new row-reading consumer adds a test whose index has a `GameKind` and a `CharacterRulesProfile` that disagree) is not followed for the rows added since the baseline. `grep -rn 'install_(spell_cast|armor_rating|fall|block)_model'` finds only the definitions and `scene_runtime.rs:79-83`: no test calls them, so the profile to resource arm and the `None => remove_resource` arm are unpinned. `install_bow_model` is covered only implicitly (default `GameKind::Fallout3NV` index with an Oblivion profile). `crime.rs`'s install tests always pass a matching game and profile (`GameKind::Oblivion` + `OBLIVION`, ...), so they never show the profile deciding. No value-pinning "row is stated on every profile" test exists for `spell_cast`, `bow`, `block`, `fall`, `armor_rating`, `stance`, `stealth`, `indicator` (the armor, crime, vats and perk rows each have one), so moving `block: Some(BlockModel::OBLIVION)` onto the Skyrim profile would break no test.
- **Evidence**: the grep above; `crime.rs:996,1011,1020-1022` pair `GameKind::{Oblivion,Skyrim,Fallout3NV,Fallout4}` with their own profiles.
- **Impact**: a future consumer-side game test, or a wrong row on a profile, would not fail a test; the consumers themselves are game-free today (0 production `GameKind` hits).
- **Related**: #6760 (closed), CHAR-D1-02.
- **Suggested Fix**: one `*_follows_the_profile_not_the_game` test per installer (Skyrim game + Oblivion profile installs; Oblivion game + FNV profile clears) and one `the_new_rule_rows_are_stated_on_every_profile` table in `profile.rs`; make the crime install test use a disagreeing pair. Optionally add the `armor.rs`-style `production_text` `GameKind::` scan to `combat.rs`, `bow.rs`, `player_cast.rs`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other CHARAL rule rows / overlays / profiles)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
