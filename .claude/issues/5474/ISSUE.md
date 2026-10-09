# #5474: TD8-2026-10-08-01: `clippy --workspace --all-targets` regressed 0 → 9 lints in three files, all from today's commits; the plugin lib-test target fails, so its downstream test targets go unlinted

**Labels**: low,tech-debt,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5474

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-10-08.md` — `TD8-2026-10-08-01` (HEAD `00f580e09`)

**Publish note**: The `byroredux (bin test)` count includes the `type_complexity` error filed separately as TOOL-CI-2026-10-08-01; the 9 lints here are the `--all-targets`-only ones.

- **Severity**: LOW. These are test and example targets, outside the CI gate.
- **Dimension**: 8 — Dead Code & Backwards-Compat Cruft (clippy, lower-priority bucket)
- **Location**:
  - `byroredux/src/systems/dialogue_voice.rs:235` (`unusual_byte_groupings`; test; `f8950e7cc`)
  - `crates/plugin/src/esm/records/misc/pack.rs:1020` (`unusual_byte_groupings`), `:1023,1030,1040` (`useless_vec` ×3),
    `:1030` (`needless_borrows_for_generic_args`). All in tests, from `287214103`.
  - `crates/plugin/examples/cond_dump.rs:10` (`format_in_format_args`) and `:13` (`print_literal`), from `00f580e09`.
- **Status**: NEW. The 10-05 baseline measured 0 after #5115.
- **Effort**: trivial
- **Evidence**:
  - Command: `cargo clippy --workspace --all-targets --keep-going -- -D warnings` on the 1.96.0 toolchain.
  - Result: `could not compile byroredux-plugin (lib test) due to 5 previous errors`, `(example "cond_dump") due to 2`,
    `byroredux (bin "byroredux" test) due to 2`. The bin-test count includes the TOOL-CI-01 `type_complexity`.
- **Impact**: #5115 brought this bucket to zero on 10-01. Without a lane it re-accumulates, and a lib-test failure stops
  linting of every dependent test target.
- **Related**: TOOL-CI-2026-10-08-01 (the main-gate red, same day), #5115 (CLOSED), TD8-2026-10-08-02.
- **Suggested Fix**:
  - Apply clippy's suggestions (`0x0000_01AB`, slices for `vec!`, drop the `&`), or delete `cond_dump.rs` (TD8-02).
  - Consider a non-blocking `--all-targets` clippy step, so the bucket stays at zero.

## Completeness Checks
- [ ] **SIBLING**: `cargo clippy --workspace --all-targets --keep-going -- -D warnings` (1.96.0) re-run to zero after the fix, so the newly-linted downstream test targets are covered
