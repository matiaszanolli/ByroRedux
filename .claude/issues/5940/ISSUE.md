# PHYS-D4-2026-10-16-01: The over-encumbrance speed multiplier is taken raw from a GMST and reaches the player translation unclamped

Labels: low,physics,gameplay,bug
Source: docs/audits/AUDIT_PHYSICS_2026-10-16.md

- **Severity**: LOW
- **Dimension**: Character & NPC Controller
- **Location**: `byroredux/src/encumbrance.rs:63-69,84-90` (source), `byroredux/src/systems/character.rs:331` (`speed_mul = (..) * encumbrance_speed`)
- **Status**: NEW
- **Description**: For Fallout 3/NV the multiplier is `index.game_setting_float("fMoveEncumEffect")` (any `f32` the load order authors, including NaN/negative/huge) and `PlayerEncumbrance::movement()` returns it verbatim when over capacity. `character_controller_system` multiplies it into `controller.move_speed` for both walking and swimming. Nothing between there and `move_character` (which does no sanitising of `desired_translation`) or the `Transform` write at `character.rs:608` checks finiteness, and `integrate_vertical`'s `v < terminal` comparison does not launder NaN.
- **Evidence**: `GameSetting` float -> `OverEncumbered::SpeedMult(mult)` -> `(mult, true)` -> `horizontal_motion(.., controller.move_speed * speed_mul, dt)` -> `new_pos = current_pos + result.translation` -> `t.translation = new_pos`. A negative multiplier reverses the controls; NaN/inf poisons the player `Transform` (and every consumer of it) until another teleport.
- **Impact**: Only a hostile/broken plugin GMST triggers it (vanilla is 0.4), and only while over-encumbered. Hardening gap, same class as #5530/#4134 (which sanitised a saved pose and capsule extents).
- **Trigger Conditions**: FO3/NV load order whose `fMoveEncumEffect` is non-finite or negative, and a player carrying more than Carry Weight.
- **Related**: #5530, #4134
- **Suggested Fix**: Sanitise at install: `.filter(|m| m.is_finite() && *m >= 0.0).unwrap_or(FALLOUT_MOVE_ENCUM_EFFECT)` in `install_encumbrance_rules` (and an upper clamp if a speed-up is not meant to be authorable).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / the sibling producer or consumer
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix

_Filed from `docs/audits/AUDIT_PHYSICS_2026-10-16.md`. Fix with `/fix-issue <number>`._
