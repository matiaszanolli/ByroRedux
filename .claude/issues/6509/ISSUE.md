# SAVE-2026-10-27-02: #6237 dropped `collision.rs` out of the shape scan, leaving `MotionType`, the payload of the saved `ReferenceMotionTypeState`, unfingerprinted

Labels: low, save-load, test-gap, bug
Source: docs/audits/AUDIT_SAVE_2026-10-27.md

**Source**: `docs/audits/AUDIT_SAVE_2026-10-27.md` finding `SAVE-2026-10-27-02` (LOW)
**Location (re-mapped to current tree)**: `byroredux/src/save_io/serde_default_guard_tests.rs` (`save_type_sources`: the `candidates.retain` arms + the explicit nested-payload list, ~:106-136); `crates/core/src/ecs/components/collision.rs` (`MotionType`, `inspect`-gated derive); `crates/scripting/src/fragment/state.rs` (`ReferenceMotionTypeState { overrides: HashMap<u32, MotionType> }`)

## Description
Residue class of closed #5059 (nested payload files outside the shape scan).
1. `save_type_sources()` keeps a file only if it contains `cfg_attr(feature = "save"` or defines a registered type name.
2. Before v116, `collision.rs` qualified through the registered `RigidBodyData`. #6237 unregistered that type, and every derive in the file is `inspect`-gated, so the file silently left the scan. The v116 baseline refresh absorbed the move along with the intended change.
3. `MotionType` is now serialized inside the newly registered `ReferenceMotionTypeState`. Its file is outside both serde guards and not on the explicit nested-payload list (`lighting.rs`, the `sdk` files) that #5059 created for exactly this shape.

## Evidence
`save_type_sources()` explicit list (verified at HEAD) contains `form_id.rs`, `components/form_id.rs`, `string/mod.rs`, `script_instance.rs`, `lighting.rs`, `sdk/identity.rs`, `sdk/script_function.rs` but not `components/collision.rs`; `MotionType` is `#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]`, and `ReferenceMotionTypeState` is `#[cfg_attr(feature = "save", derive(...))]` holding `HashMap<u32, MotionType>`.

## Impact
Renaming or removing a `MotionType` variant changes no fingerprint and demands no `FORMAT_MAJOR` bump. Every save holding a scripted `SetMotionType` override would then fail the typed preflight ("save load ABORTED"), leaving that save unloadable. The enum is small and stable, hence LOW. No CI signal.

## Related
#5059, #6237, #5403 (coverage markers precedent), #2379.

## Suggested Fix
Add `manifest.join("../crates/core/src/ecs/components/collision.rs")` to the explicit list (with a #6237 comment) and to `source_discovery_follows_registry_and_nested_save_modules`'s suffix list; refresh the baseline without a bump (no saved shape changed). Optionally add a coverage marker asserting `MotionType` contributes a shape, like #5403's `StoryManagerNodeState` markers.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / sibling systems
- [ ] **TESTS**: A regression test pins this specific fix

