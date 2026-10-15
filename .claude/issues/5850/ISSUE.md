# TD8-2026-10-15-02: `--all-targets` (non-blocking CI step) carries 26 test/example-target lints; the 10-12 deny-level HKX error is gone

Labels: low,tech-debt,bug

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-10-15.md` (finding `TD8-2026-10-15-02`).

- **Severity**: LOW
- **Dimension**: 8 - Dead Code & Backwards-Compat Cruft (clippy, test targets)
- **Location**: `crates/hkx/examples/{annotation_census_ba2,behavior_census,behavior_census_ba2,class_census_ba2,graph_dump,sound_events,clip_info}.rs` (10, mostly `needless_borrow` at `path`), `crates/hkx/src/behavior.rs:1740` (`vec_init_then_push`); `crates/scripting/examples/alias_script_coverage.rs:95,104,108` and `crates/plugin/examples/qust_alias_script_census.rs:88` (`unnecessary_sort_by`); `crates/scripting/src/fragment/vm/tests.rs:905,906,5018`, `crates/scripting/src/condition.rs:2056` (`clone_on_copy`), `crates/scripting/src/animation_graph.rs:1838` (`needless_borrow`); `crates/renderer/src/vulkan/groundcover.rs:1471-1473` (3x `assertions_on_constants`), `crates/ui/src/prepare.rs:695` (`assertions_on_constants`), `crates/plugin/src/esm/records/soun.rs:185` (`needless_borrows_for_generic_args`), `byroredux/src/systems/graph_locomotion.rs:344,359` (`manual_range_contains`)
- **Status**: NEW bucket (10-12 had 11 + 1 deny; `TD8-2026-10-12-02` / #5744 closed and verified: `KNOWN_VANILLA_REJECTS` assert no longer trips `absurd_extreme_comparisons`)
- **Age**: 3 days or less for all sampled (hkx example census set `3bb6806b5`, groundcover constants `af8323881`, ui `d601b45ab`)
- **Effort**: small
- **Description**: `cargo clippy --workspace --all-targets --keep-going` (no `-D`) finishes; 49 diagnostics total, 23 of them duplicates of the production lints in TD8-01, the remaining 26 only exist in test/example targets. `assertions_on_constants` x4 (renderer + ui) are tests asserting compile-time constants — they should be `const _: () = assert!(…)`.
- **Evidence**: `/tmp/audit/tech-debt/d8_clippy_all.txt`, `all_lints.txt`.
- **Impact**: CI step is `continue-on-error`, so it is informational; no deny-level error at HEAD.
- **Related**: #5621 (closed), TD8-2026-10-15-01.
- **Suggested Fix**: `cargo clippy --fix --workspace --all-targets` handles most; convert the four constant assertions to `const` asserts.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files/docs
- [ ] **TESTS**: A regression test or guard pins this specific fix (or the existing guard is named)
