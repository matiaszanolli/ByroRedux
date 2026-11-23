# #7296: SCR-D1-02: Doc and skill text still call `Game.ShowRaceMenu` an auto-accepting placeholder

- **Severity**: LOW
- **Dimension**: Recognizer Chain (placeholder accounting)
- **Untrusted-Input**: No
- **Location**: `docs/engine/ui.md:899-902`; `crates/scripting/src/translate/effects/model.rs:1133-1134`; `crates/scripting/src/obscript_more.rs:105-106`; `.claude/commands/audit-scripting/SKILL.md` Dim 1 "Placeholders are coverage with a hole"
- **Status**: NEW (comment and doc rot)
- **Description**: `bcfe6b5dd` made `Effect::ShowRaceMenu` a real effect. The placeholder set is now `ShowAppearanceMenu`, `RequestSave`, `SetHudCartMode` and `AddAchievement`. The pin `only_the_counter_only_primitives_are_placeholders` agrees with `is_placeholder` (verified below). Four places still describe the old state.
- **Evidence**:
  - `ui.md:900-902`: "`menu RaceSex Menu (S4, accepts the current appearance) from ShowRaceMenu`). `Game.ShowRaceMenu`'s auto-accept placeholder is that fallback."
  - `model.rs:1133`: "a `ShowRaceMenu` inside an `If` is still a stub claimed as coverage (#5975)"
  - `obscript_more.rs:105`: "What the effect already did in the menu's place (`Game.ShowRaceMenu` accepts the current appearance)"
  - SKILL.md Dim 1: "`Effect::is_placeholder` (`ShowRaceMenu`, `RequestSave`, `SetHudCartMode`, `AddAchievement` …)".
- **Impact**: A reader of the coverage harness or the skill believes a ShowRaceMenu inside an `If` is reported as a stub. It is not. The live registry report now only names `ShowAppearanceMenu`.
- **Related**: #4328, #5975
- **Suggested Fix**: Edit the four texts. In ui.md, use the appearance report (`menu LooksMenu (S4, no appearance editor until the slider milestone (GUI-NG-005))`) as the S4-with-fallback example.

**Found by**: `docs/audits/AUDIT_SCRIPTING_2026-11-22.md`

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other latent / suspending paths, the other drain owners, the other ObScript hosts)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved and the `apply_effect` lock inventory is updated
- [ ] **TESTS**: A regression test pins this specific fix
