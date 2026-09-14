# #4322 SCR-D5-2026-09-14-02: `Effect::SetInChargen` fields and the persisted `CinematicPresentationState` flags carry invented semantics

**Labels**: medium,scripting,save-load,bug,game:skyrim
**Source**: `docs/audits/AUDIT_SCRIPTING_2026-09-14.md`

- **Severity**: MEDIUM
- **Dimension**: Recognizer-Chain Soundness
- **Untrusted-Input**: No
- **Location**: `crates/scripting/src/translate/effects.rs` `Effect::SetInChargen` + `prim_set_in_chargen`; `crates/scripting/src/cinematic.rs` `in_chargen` field doc and `set_in_chargen`; `crates/save/src/snapshot.rs` (FORMAT v23)
- **Status**: NEW
- **Description**: The real signature is `Game.SetInChargen(Bool abDisableSaving, Bool abDisableWaiting, Bool abShowControlsDisabledMessage)`, the same in Skyrim `game.pex` and FO4 `Game.psc:312`. The code documents it as `(abEnabled, abWaitForRaceSex, abStayInFirstPerson)`. The values are kept in the right positions, but every label is wrong, and the real semantics (block save and wait) are enforced nowhere, so the lowering claims a statement it doesn't model.
- **Evidence**: The orchestrator re-ran the probe on `game.pex`: `Game.SetInChargen(Bool abDisableSaving, Bool abDisableWaiting, Bool abShowControlsDisabledMessage) flags=0x3 [global native]`. The string table has none of the three names the code uses.
- **Impact**: No reader today. The wrong names are now baked into save format v23 and a public resource, so the first consumer (the documented future "chargen camera/input mode") will read disable-saving as "in chargen". Saving and waiting are not blocked during MQ101 chargen as vanilla requires.
- **Related**: SCR-D5-2026-09-14-01
- **Suggested Fix**: Rename the fields and docs to `disable_saving` / `disable_waiting` / `show_controls_disabled_message` before a consumer lands, ideally in the same format bump. Add a non-literal-bool decline test.

_Source: `docs/audits/AUDIT_SCRIPTING_2026-09-14.md`_

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other primitives / spawn paths / walkers)
- [ ] **TESTS**: A regression test pins this specific fix
