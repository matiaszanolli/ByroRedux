# #7288: SCR-D1-01: Fallout 4 `Game.ShowRaceMenu(akMenuTarget, 0, …)` lowers on `uiMode` alone and drops a non-player `akMenuTarget`; the engine's race menu then edits the player

- **Severity**: HIGH (per the skill's escalation: a lowering that accepts an unmodeled term instead of declining. Exposure is low: vanilla FO4 passes `None` or the player.)
- **Dimension**: Recognizer Chain
- **Untrusted-Input**: Yes (the call arrives from a decompiled mod `.pex`; the value is the receiver expression)
- **Location**: `crates/scripting/src/translate/effects/prims/player_control.rs:120-146` (`prim_show_race_menu`); runtime `crates/scripting/src/fragment/effects/deferred.rs:998-1016` (`flush_cinematic_presentation`)
- **Status**: NEW (the umbrella #6667, "M48 Phase 5: interactive race and sex selection menu", is open but is about the UI, not the recognizer)
- **Description**: The FO4 CK page (`falloutck-uesp-wiki/(main)/ShowRaceMenu - Game.wiki`) gives `akMenuTarget` as "The primary actor to be modified by the race menu. Default: None (use the Player)". `akMenuSpouseFemale` and `akMenuSpouseMale` are "used in the character generation version of the race menu", which is mode 0. The new primitive rejects only a *literal* in the reference slots (`Int/Float/Bool/StringLit`). Any identifier, property or call (a companion property, `Self.GetActorRef()`) passes, and so does `Game.GetPlayer()`. Mode 0 then lowers to the argument-less `Effect::ShowRaceMenu`. Its runtime pushes `MenuRequest { menu: "race", .. }` and the character-creation page edits the player. The doc comment justifies the drop with "with no editor they are not read". That holds for modes 1-4 (S4, no editor) but not for mode 0, where an editor opens. The primitive-level rule is: accept only the literal-default form of a parameter, decline the rest. The target and spouse slots accept `None` or a player expression and decline anything else.
- **Evidence**:
  ```rust
  // player_control.rs:130-135
  let literal = |slot: usize| {
      args.get(slot).is_some_and(|arg| matches!(arg.value.node, Expr::IntLit(_) | Expr::FloatLit(_) | Expr::BoolLit(_) | Expr::StringLit(_)))
  };
  if [0, 2, 3, 4].into_iter().any(literal) { return None; }
  ...
  // :140-145
  0 => Some(Effect::ShowRaceMenu),
  mode @ 1..=4 => Some(Effect::ShowAppearanceMenu { mode: mode as u8 }),
  ```
  The new test `fallout4_show_race_menu_lowers_on_its_mode` (`translate/effects/tests.rs`, near line 3139) passes only `None` in slots 0, 2, 3 and 4. No test covers a non-`None` reference.
- **Impact**: A mod `.pex` that calls `Game.ShowRaceMenu(SomeCompanion, 0)` gets a lowered effect. The player's face and sex are edited instead of the companion's, with no VM fallback, because the recognizer claimed the call. A wrongly lowered script corrupts state and an inert one does not.
- **Related**: #6667, #4328 (placeholder accounting). For 1-4 the target is also dropped, but that effect is a documented placeholder (`Effect::is_placeholder`).
- **Suggested Fix**: Lower only when slot 0 is absent, `None` or a player expression (`Game.GetPlayer()` or the player-alias property, through the helper the other player-receiver primitives use), and decline otherwise. Do the same for the spouse slots at mode 0 until the page shows a spouse. Add a decline test with a non-`None` actor argument.

**Found by**: `docs/audits/AUDIT_SCRIPTING_2026-11-22.md`

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other latent / suspending paths, the other drain owners, the other ObScript hosts)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved and the `apply_effect` lock inventory is updated
- [ ] **TESTS**: A regression test pins this specific fix
