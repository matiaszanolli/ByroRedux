# #6985: TD8-2026-11-15-01: The blocking `cargo clippy --workspace -- -D warnings` gate is red on `movt.rs:37`, a sixth recurrence of the class

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/6985
- **Labels**: low,esm-plugin,tech-debt,bug
- **Source**: docs/audits/AUDIT_TECH_DEBT_2026-11-15.md

From the tech-debt audit `docs/audits/AUDIT_TECH_DEBT_2026-11-15.md` (HEAD 995ed089b).

- **Severity**: LOW (no promotion trigger applies; note it blocks CI)
- **Dimension**: 8 - Dead Code & Backwards-Compat Cruft (clippy gate)
- **Location**: `crates/plugin/src/esm/records/movt.rs:35-39`
- **Age**: `49c92afbc` 2026-11-15 (Fix #6910, M65 Phase 1, which created `movt.rs`); committed text, not in-flight
- **Effort**: trivial
- **Labels**: tech-debt
- **Status**: NEW. Regression class of #6222 / #6582 (both CLOSED, "the blocking clippy gate is red again"); #5308 is the same lint (`chunks_exact_to_as_chunks`)
- **Description**: `cargo clippy --workspace --keep-going -- -D warnings` at HEAD (rustc 1.100.0) exits 101 with exactly one error: `using chunks_exact with a constant chunk size` at `movt.rs:37`. `byroredux-plugin` (lib) fails to compile under `-D`, so under the gate the crates downstream of plugin go unlinted. I re-ran the downstream crates without `-D` (see TD8-02's lane) and they carry no lib-target lints, so nothing else is hidden today.
- **Evidence**:
  ```
  error: using `chunks_exact` with a constant chunk size
    --> crates/plugin/src/esm/records/movt.rs:37:22
  37 |                     .chunks_exact(4)
     = note: `-D clippy::chunks-exact-to-as-chunks` implied by `-D warnings`
  ```
- **Impact**: CI `Test + Check + Clippy` fails on every push until fixed; the gate was at zero after 3c9949ec9 (2026-11-14).
- **Related**: TD8-02 (same commit), #6222, #6582, #5308.
- **Suggested Fix**: `record.speeds = sub.data.as_chunks::<4>().0.iter().map(|b| f32::from_le_bytes(*b)).collect();` (the form 3c9949ec9 used for the other 140 sites).
- **Re-checked by the orchestrator at 995ed089b**: `movt.rs:37` still reads `.chunks_exact(4)`; the gate is still red.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or an extended guard) pins this specific fix
