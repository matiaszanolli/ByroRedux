# #7184 — TOOL-D3-2026-11-21-02: the actor-value projection's #7044 `stored` fold has no test

Labels: low,tech-debt,bug,test-gap

- **Severity**: LOW
- **Dimension**: SDK Contract
- **Exposure**: an extension with `byro.actor-values.read`, on an Oblivion actor carrying stored level-up Health gains (save v136)
- **Location**: `byroredux/src/extensions/capture.rs` (folds `value.base + value.set_override + value.stored` into the SDK `base`); `byroredux/src/extensions/tests.rs` (the #5508 test)
- **Status**: NEW
- **Description**: the #5508 test covers the `set_override` layer only. Dropping `+ value.stored` would make the projection's `current()` fall short of the live `ActorValue::current()` with no test failing.
- **Suggested Fix**: extend the #5508 test with a stored Health gain, asserting that the projection's `current()` equals the live one, or project through `ActorValue::undamaged()` so the layers cannot drift.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other writers / readers / folds named above)
- [ ] **TESTS**: A regression test pins this specific fix

Source: `docs/audits/AUDIT_TOOLING_2026-11-21.md`
