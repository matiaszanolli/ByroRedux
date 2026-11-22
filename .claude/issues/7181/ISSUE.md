# #7181 — TOOL-D3-2026-11-21-01: VATS press and release never reach extensions or the Papyrus input view, although every contract layer advertises `vats`

Labels: medium,tech-debt,scripting,bug

- **Severity**: MEDIUM. An advertised mod-facing contract is not delivered, silently.
- **Dimension**: SDK Contract
- **Exposure**: modder. It affects any extension whose manifest subscribes to `byro.input.action` and filters on `"vats"`, and any Papyrus `Input.GetMappedKey("VATS")` / `IsKeyPressed` path.
- **Location**: `byroredux/src/interaction.rs` `InputAction::OBSERVABLE` (`[Self; 14]`). It is consumed by `extension_input_dispatch_system` (`byroredux/src/extensions/systems.rs`) and by `papyrus_bindings` (`interaction.rs`).
- **Status**: NEW (sibling of #6528, closed, which fixed only the `parse` side)
- **Description**:
  - `InputAction::Vats` is bound to V and read by `systems/vats.rs`.
  - It maps to `SdkInputAction::Vats` in `sdk_input_action` (`extensions/mod.rs`), is in the WIT `input-action` enum, and parses from `"vats"`.
  - `OBSERVABLE` holds the 16 SDK actions minus `Pause` and `Vats`. The source documents why `Pause` is absent (it has no physical source); nothing says why `Vats` is.
  - 818eba7f4 added `Vats` to the map, the key binding and `papyrus_control_name`, but not to `OBSERVABLE`. Sneak (49c92afbc) and ReadyWeapon (533f1f644) were later added to it; Vats never was.
- **Evidence**: re-read by the coordinator. `OBSERVABLE` lists MoveForward…ReadyWeapon (14 actions) with no `Vats`, while `extensions/mod.rs` maps `InputAction::Vats => SdkInputAction::Vats`.
- **Impact**: an extension that waits for the player to open VATS never hears it, and a ported SKSE script's VATS control mapping reads as unbound.
- **Suggested Fix**: add `Self::Vats` to `OBSERVABLE`, or document why it is withheld. Add a test that every engine `InputAction` is either in `OBSERVABLE` or in `sdk_input_action`'s explicit `None` arm (or `Pause`), so a new action cannot compile in unobservable.

### LOW

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other writers / readers / folds named above)
- [ ] **TESTS**: A regression test pins this specific fix

Source: `docs/audits/AUDIT_TOOLING_2026-11-21.md`
