# #7310: SCR-D6-05: Two skill-use rule claims rest only on a disassembly

- **Severity**: LOW
- **Dimension**: Legacy ObScript (owner rule: no rule sourced from the executable)
- **Untrusted-Input**: No
- **Location**: `crates/scripting/src/oblivion_skill_use.rs:236-265` with `obscript_more/state.rs:34-39` (`AdvancePCSkill` keeps "the executable's banked − cost, clamped at 0"); `crates/core/src/character/skill_use.rs:230-232` (`ARMOR_HIT_ATTEMPTS = 7`, "`0x5e5a00` loops seven times") with `byroredux/src/skill_use_emit.rs:77-98`
- **Status**: NEW (a pending owner question per the 2026-11-20 ruling already covers the player-spellcasting numbers; these two are in Dim 6 code and not listed there)
- **Description**:
  - *`AdvancePCSkill` surplus.* CS *ModPCSkill* documents two behaviours: unpatched `usage -= old limit` and Beta Patch 1.1 "setting your uses value to 0.0"; PRD Q5 chose the patched one for `ModPCSkill`. CS *AdvancePCSkill* says only "as if the player earned the skill by use or training. This function is subject to the same issue as ModPCSkill". The engine gives `AdvancePCSkill` a third behaviour (`(banked - cost).max(0.0)`), and its comment names the executable as the source. Neither the unpatched nor the patched page behaviour matches it.
  - *Armor re-roll count.* CS *iArmorDamageXXChance* documents the five weights (defaults 10/25/15/10/10, "percentage of damage absorbed"); UESP *Armor* says "the exact mechanism for determining which armor pieces you are wearing take how much damage is unclear" and UESP *Light Armor* says only that a hit on covered body earns 1.5. The "roll, then re-roll up to seven times on an empty slot" mechanism and the shield-as-remainder come from the `0x5e5a00` decode (`skill-use.md` §3 "decoded this phase"), dated before the ruling.
- **Evidence**: `banked = if keep_surplus { (banked - cost).max(0.0) } else { 0.0 };` and `pub const ARMOR_HIT_ATTEMPTS: usize = 7;`.
- **Impact**: low and bounded (a script-only command; a Light/Heavy Armor split for mixed outfits), but the behaviour is presented as a rule rather than a declared gap.
- **Suggested Fix**: follow the ruling: for `AdvancePCSkill`, either adopt the Q5 behaviour (0.0) because CS ties it to `ModPCSkill`, or record the surplus rule as an owner decision; for the armor roll, state in `skill-use.md` that only the weights are documented and that the attempt count and shield remainder are an engine choice, or re-source them.

**Found by**: `docs/audits/AUDIT_SCRIPTING_2026-11-22.md`

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other latent / suspending paths, the other drain owners, the other ObScript hosts)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved and the `apply_effect` lock inventory is updated
- [ ] **TESTS**: A regression test pins this specific fix
