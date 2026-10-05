# #5327: TD8-2026-10-05-02: The CI `cargo check -p byroredux --no-default-features` lane builds with two warnings: `debug_server_allowed` is dead without the feature, and `scheduler` needs no `mut`

Labels: low,tech-debt,bug
Filed from: docs/audits/AUDIT_TECH_DEBT_2026-10-05.md

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-10-05.md` (TD8-2026-10-05-02) · audit HEAD `a2c24b16e`

- **Severity**: LOW
- **Dimension**: 8 — Dead Code & Backwards-Compat Cruft
- **Location**: `byroredux/src/main.rs:78-80` (`fn debug_server_allowed`), `:1051` (`let mut scheduler`); the
  lane is `.github/workflows/ci.yml:172`
- **Status**: NEW
- **Age**: `debug_server_allowed` came with `63c0aee3b` (2026-09-27). Its only production caller is under
  `#[cfg(feature = "debug-server")]` (`main.rs:1069-1070`).
- **Effort**: trivial
- **Evidence**: reproduced locally on rustc 1.96:
  ```
  warning: variable does not need to be mutable  --> byroredux/src/main.rs:1051:13
  warning: function `debug_server_allowed` is never used  --> byroredux/src/main.rs:78:4
  ```
- **Impact**:
  - The lane is not `-D warnings`, so it stays green while the non-default build accumulates warnings.
  - That is the slow-rot shape the feature-lane rule exists to catch (#3894/#4387).
  - A future `-D warnings` on this lane, or a clippy run with `--no-default-features`, fails straight away.
- **Suggested Fix**:
  - Gate `debug_server_allowed` with `#[cfg(any(test, feature = "debug-server"))]`.
  - Make the `mut` conditional by rebinding inside the cfg block.
  - Consider `RUSTFLAGS=-D warnings` on the feature-lane `cargo check` steps.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix
