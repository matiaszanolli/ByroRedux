# #6986: TD8-2026-11-15-02: The non-blocking `--all-targets` lane is red: four deny-level `approx_constant` errors stop `byroredux-core`'s lib-test target linting, plus one `field_reassign_with_default`

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/6986
- **Labels**: low,character,test-gap,tech-debt,bug
- **Source**: docs/audits/AUDIT_TECH_DEBT_2026-11-15.md

From the tech-debt audit `docs/audits/AUDIT_TECH_DEBT_2026-11-15.md` (HEAD 995ed089b).

- **Severity**: LOW
- **Dimension**: 8 (clippy lane)
- **Location**: `crates/core/src/character/stance.rs:157-158` (four errors), `crates/scripting/src/stance.rs:122-123` (one warning); the `movt.rs:37` warning of TD8-01 repeats here
- **Age**: all from `49c92afbc` 2026-11-15
- **Effort**: trivial
- **Labels**: tech-debt, test-gap
- **Status**: NEW (class of #6193 / #6492, both CLOSED)
- **Description**: `cargo clippy --workspace --all-targets --keep-going` (no `-D`) exits 101. `byroredux-core` (lib test) fails with four `clippy::approx_constant` errors, which are deny-level and so fail even without `-D`. The test table `skyrim_speeds` writes `1.57, 3.14, 3.14` (rotation speeds) where clippy reads `3.14` as an approximate `PI`. `byroredux-scripting` (lib test) warns `field_reassign_with_default`, where `controls` is built with `PlayerControlState::default()` and then has `sneaking_enabled` assigned.
- **Evidence**:
  ```
  error: approximate value of `f{32, 64}::consts::PI` found
     --> crates/core/src/character/stance.rs:157:105   (also :157:111, :158:101, :158:107)
  warning: field assignment outside of initializer ... crates/scripting/src/stance.rs:123:9
  ```
- **Impact**: core's own unit tests are not clippy-checked until fixed, and the lane's "expected zero" baseline is broken one day after it was re-zeroed.
- **Suggested Fix**: the `3.14` values are Skyrim SPED data, not a PI approximation. Add `#[allow(clippy::approx_constant)] // authored Skyrim SPED values, not an approximation of PI` on `skyrim_speeds`, or write them as `3.140_001` / `std::f32::consts::PI` if the test only checks ratios. For the scripting site use `PlayerControlState { sneaking_enabled: false, ..Default::default() }`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or an extended guard) pins this specific fix
