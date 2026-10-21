# #6322: LC-2026-10-21-D1-02: `mesh.info` prints a glam ZYX decomposition of the Y-up quaternion as if it were the REFR `DATA` triple

**Labels**: low,bug,legacy-compat,tech-debt

**Source**: docs/audits/AUDIT_LEGACY_COMPAT_2026-10-21.md · **Severity**: LOW · **Dimension**: 1 — REFR Euler (diagnostic surface) · **Location**: `byroredux/src/commands/assets.rs:659-676`; `docs/engine/debug-cli.md:553-555` (which also names the wrong file, `commands/mod.rs`)

- **Severity**: LOW
- **Dimension**: 1 — REFR Euler (diagnostic surface)
- **Location**: `byroredux/src/commands/assets.rs:659-676`; `docs/engine/debug-cli.md:553-555` (which also names the wrong file, `commands/mod.rs`)
- **Status**: NEW (the code dates from `9114da751`, 2026-05-26; no issue or audit mentions it)
- **Description**: The comment says the readout "matches the FNVEdit / CK convention the user sees in the ESM (REFR DATA stores RX RY RZ as ZYX-composed Euler)". The code is `t.rotation.to_euler(EulerRot::ZYX)` on the engine's Y-up quaternion, printed as `rx/ry/rz`. These are angles about Y-up axes in glam's `Rz·Ry·Rx` order, not the Z-up REFR triple, under either the old or the new ship mode.
- **Evidence**: A REFR with only a heading `rz = θ` spawns as `Ry(−θ)` in Y-up (true in both modes). `to_euler(ZYX)` returns `(z=0, y=−θ, x=0)`, so `mesh.info` prints `rx=0 ry=−θ rz=0`: the heading shows on the wrong axis with the wrong sign. `quat_yup_to_euler_zup` (added in `d4bfe6abc`) is now the exact inverse of the shipping conversion.
- **Impact**: This is a diagnostic only, but it is step 3 of `docs/engine/geometry-defect-triage-workflow.md` (`mesh.info` on the picked ref). An operator comparing its rotation with FNVEdit's REFR `DATA` sees a mismatch on every rotated REFR, including correctly placed ones. That pushes them toward the false "rotation bug" conclusions the `--rotation-mode` dispute produced twice.
- **Related**: `2a8b8e505`, `d4bfe6abc`.
- **Suggested Fix**: Print `quat_yup_to_euler_zup(t.rotation)` in degrees and label it "REFR DATA convention". Correct `debug-cli.md:553-555` and its file path.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix

_Source: `docs/audits/AUDIT_LEGACY_COMPAT_2026-10-21.md` (HEAD 4a3e89d56). Fix with `/fix-issue <number>`._
