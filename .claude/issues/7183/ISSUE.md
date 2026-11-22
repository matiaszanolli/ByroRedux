# #7183 — TOOL-D2-2026-11-21-01: the registry-completeness guard sees only `crates/core`, and scripting/physics state holders are unregistered

Labels: low,tech-debt,bug,test-gap

- **Severity**: LOW
- **Dimension**: Protocol & Registry
- **Exposure**: developer (`byro-dbg` component inspection)
- **Location**: `crates/debug-server/src/registration.rs` `every_inspectable_component_is_registered` (walks `crates/core/src` for the `inspect` derive)
- **Status**: NEW (the "remaining gap class" the skill names; #4755 and #5694 cover core only)
- **Description**: components outside core carry a `save`-feature serde derive instead, the route `SpellList` took to registration. A scan of the ~160 `impl Component` outside core finds these with that derive and no `register_component::<T>`:
  - `crates/scripting`: `ActiveMagicEffects`, `Bleedout`, `ReadiedSpell` (`cast.status` covers it), `ScriptVariables`, `TwoStateActivator` (`door.state` partly covers it), `WeaponDrawn`, `ActorControlState`, `ScriptTimer`, `ActorCinematicState`, `HorseTetherState`, `SpeakingLine`, `IgnoreCrime` and `ScriptPackage`;
  - `crates/physics`: `CharacterController`;
  - the demo: `RumbleOnActivate`.

  Nothing fails when one is added.
- **Evidence**: `ActiveMagicEffects` carries `cfg_attr(feature = "save", derive(serde::Serialize, …))`, like the registered `SpellList`, and has no `register_component::<ActiveMagicEffects>`. Which of them deserve registration is a judgement call.
- **Suggested Fix**: extend the roster scan to `crates/scripting/src` and `crates/physics/src` (serde derive in the `save` window plus `impl Component`), with an explicit allowlist for deliberate omissions (event batches, request markers, demo components). Register the state holders a developer inspects (`ActiveMagicEffects`, `Bleedout`, `ScriptVariables`, `WeaponDrawn`), and update the doc table and count.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other writers / readers / folds named above)
- [ ] **TESTS**: A regression test pins this specific fix

Source: `docs/audits/AUDIT_TOOLING_2026-11-21.md`
