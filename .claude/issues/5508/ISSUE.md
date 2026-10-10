# #5508: SAVE-D2-2026-10-09-01: #5412's new `ActorValue.set_override` layer is folded into `current()` but missed by three readers that re-sum the layers by hand: the HUD bar fraction, the debug vitals, and the SDK actor-value projection

**Labels**: bug, character, medium, tech-debt, ui

**Source**: `docs/audits/AUDIT_SAVE_2026-10-09.md` — finding `SAVE-D2-2026-10-09-01` · suite `/audit-suite --preset streaming-deep` (2026-10-09) · HEAD `3bcf6c8e8`

- **Severity**: MEDIUM
- **Dimension**: Format & Schema Discipline. This is a cross-domain finding, traced from the v33→v34 field. The read sites belong to `/audit-character`, `/audit-ui` and `/audit-tooling`, none of which runs in this suite.
- **Data-Loss Class**: none (the field round-trips correctly; this is read-side only)
- **Location**:
  - `byroredux/src/hud.rs:849` (`fraction()`, shared by the MenuXml and Scaleform HUD drivers): `let max = entry.base + entry.permanent_mod + entry.temporary_mod;`
  - `byroredux/src/inventory.rs:317` (`vitals_snapshot()`, the debug-UI vitals): same `max`, and `current: max - entry.damage`.
  - `byroredux/src/extensions/capture.rs:235-240` builds `ActorValueState::new(base, permanent_mod, temporary_mod, damage)`. `crates/sdk/src/actor_values.rs:14-48` has no override slot, and its `current()` is `base + permanent + temporary - damage`.
- **Status**: NEW. It is a regression introduced by the #5412 fix (`a614eb273`). Before #5412, the player `SetBase` value lived in `permanent_mod`, which all three readers include.
- **Trigger Conditions**: `setav <player> Health|AP <v>` (console `edit_av`), or an SDK `SetBase` on a player `PlayerOnly` derived pool. Only these two writers reach `set_override` (`extensions/commands.rs:446-460`).
- **Description**: #5412 added a fifth layer: `current = base + set_override + permanent + temporary − damage` (`crates/core/src/ecs/components/actor_values.rs:78`). The commit's own test shows the effect: after `setav Health 500`, the values are base 105, `set_override` 500, current 605. The three hand-composed readers then go wrong as follows:
  - **HUD**: the bar's max stays at 105, so `current/max` clamps to 1.0. The bar shows full health until damage exceeds 500 (the old ratio was 405/605 = 0.67 after 200 damage).
  - **Debug vitals**: they report 105 − damage, which goes negative while the player actually has 405.
  - **SDK**: `EntityProjection::actor_value(Health).current()` returns the pre-`SetBase` value. A mod reading back its own `SetBase` sees it as not applied.
- **Evidence**: The grep `permanent_mod\s*\+` finds exactly these sites plus `CharacterRuleset::actor_value` (`ruleset.rs:169`). That last one is correct as written, because `set_override` is written only on `PlayerOnly` pools, and that arm composes `ActorGeneral` formulas.
- **Impact**: The player's Health and AP bars, the debug vitals panel and the SDK projection are wrong after any console or SDK `SetBase` on a derived pool. The bar under-reports damage, the panel can show negative health, and the SDK value is stale. Nothing is lost from a save.
- **Related**: #5412, #5239, #4675.
- **Suggested Fix**:
  - Give `ActorValue` an undamaged-max helper (`base + set_override + permanent_mod + temporary_mod`) and use it in both HUD readers.
  - Add the layer to the SDK `ActorValueState` (or fold it into `base` at the projection boundary). Pin it with a projection test after a routed `SetBase`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other walkers / record decoders / games / call sites)
- [ ] **TESTS**: A regression test pins this specific fix
