# #7294: SCR-D6-02: Sneak banks experience when nobody is nearby

- **Severity**: MEDIUM
- **Dimension**: Legacy ObScript (skill-use banking gates, `docs/engine/skill-use.md`)
- **Untrusted-Input**: No
- **Location**: `byroredux/src/skill_use_emit.rs:176-183` (`stance_of`) and `:209-212` (`undetected`)
- **Status**: NEW
- **Description**: UESP *Oblivion:Sneak* (local dump `uesp-wiki/Oblivion/Sneak.wiki` line 20): "You gain 0.75 experience points every second (real time) spent moving in sneak mode without being detected, **but only if there is a person or creature nearby**. No experience is gained when sneaking with nobody nearby". The emitter reads "undetected" as `PlayerDetection.level < Noticed` and takes a missing resource as undetected too. `DetectionLevel` defaults to `Lost`, so with no detector in range the stance fires and Sneak banks. `skill-use.md` §3 records that the executable's own gate (a flag at `player+0x5a8`) was "not traced" and that the engine substitutes the detection level. The documented "someone nearby" half of the rule is not in the PRD (OSL-USE-005), `skill-use.md` or the code.
- **Evidence**:
  ```rust
  let undetected = world
      .try_resource::<crate::detection::PlayerDetection>()
      .is_none_or(|detection| detection.level < DetectionLevel::Noticed);
  ...
  } else if sneaking && undetected { Some(Stance::Sneak) }
  ```
  `a_held_key_banks_no_stance_use_while_the_body_cannot_move` and the stance tests never put the player in an empty cell with sneak held.
- **Impact**: Sneak can be trained anywhere, empty rooms included, which UESP names as the thing that does not work; that is the main Sneak power-levelling exploit the rule exists to stop. It also feeds the level-up tallies (Agility).
- **Suggested Fix**: gate the Sneak stance on a documented "someone could detect the player" condition from the detection module (an actor within its detection range). If the engine cannot answer it, name the gap in `skill-use.md` §3 under "Not fed, and why" and decline the stance rather than guess.

**Found by**: `docs/audits/AUDIT_SCRIPTING_2026-11-22.md`

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other latent / suspending paths, the other drain owners, the other ObScript hosts)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved and the `apply_effect` lock inventory is updated
- [ ] **TESTS**: A regression test pins this specific fix
