# #7205 CHAR-2026-11-21-L-08: `ActorValueView::Base` / `Permanent` and `GetPermanentActorValue` leave out the `stored` layer, and the two branches of `actor_value_view` disagree

**Labels**: low,character,scripting,bug,game:oblivion

From `docs/audits/AUDIT_CHARACTER_2026-11-21.md` (`/audit-character`, HEAD `a06d5f9db`).

- **Reported as**: CHAR-D4-03
- **Severity**: LOW (latent: no live Oblivion reader today)
- **Dimension**: Population Boundary
- **Game**: Oblivion (the only game with a non-zero `stored`)
- **Location**: `crates/scripting/src/condition/eval.rs:487-489` (authored fast path: `Base => base + set_override`, `Permanent => base + set_override + permanent_mod`), `crates/scripting/src/obscript_more/state.rs:643` (`cmd_11ef`: `base + set_override + permanent_mod`); contrast the fall-through branch (`eval.rs:495-519`) which clones the entry and so keeps `stored` through `CharacterRuleset::actor_value` (`ruleset.rs:171`, `current() - base`)
- **Status**: NEW
- **Source**: `docs/engine/charal-oblivion-ruleset.md:1123-1210`: the executable's base of Health is `(stored + derived) x multiplier` (`0x5ead00`), i.e. `stored` is part of the base, and the engine's own `ActorValue::undamaged` counts it (`actor_values.rs:84-89`, #7044).
- **Description**: #7044 added `stored` to `undamaged()`, the SDK fold and the finite check, but the hand-written view arms still sum the older layers. For a `base_authored` Health entry (the Oblivion player's refreshed pool) the Base/Permanent views therefore exclude the level-up gains, while the same function's fall-through composition includes them.
- **Evidence**: the Oblivion `GetBaseActorValue` (CTDA 277) is declined on purpose (#7038, `obscript_cmd.rs:181`), and `GetPermanentActorValue` is Fallout-only, so no Oblivion script or condition can observe the divergence at HEAD.
- **Impact**: becomes a wrong `GetBaseActorValue Health` after any level-up the moment #7038's decline is lifted; until then none.
- **Related**: #5508 / #5706 (hand-summed layers class), #7038, #7044.
- **Suggested Fix**: derive Base and Permanent from `ActorValue` accessors (as `undamaged()` does) instead of summing fields, and add a test with a non-zero `stored` for each view.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other CHARAL rule rows / overlays / profiles)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
