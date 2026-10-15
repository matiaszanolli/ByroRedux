# TD9-2026-10-15-04: 44 new examples (2 680 LOC, +35 % in three days) accreted under crates whose `cargo test` links every one; the hygiene guard only sees the `tmp_` prefix and the "throwaway" marker

Labels: low,test-gap,tech-debt,bug

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-10-15.md` (finding `TD9-2026-10-15-04`).

- **Severity**: LOW
- **Dimension**: 9 - Test Hygiene
- **Location**: `crates/hkx/examples/` (17 new, 13-164 LOC each), `crates/plugin/examples/` (16 new, 20-210 LOC), `crates/scripting/examples/` (6 new + `common/archives.rs`), `crates/nif/examples/skeleton_tree_dump.rs`, `crates/pex/examples/pex_dump_fn.rs`; guard `byroredux/src/workspace_hygiene_tests.rs:241-318`
- **Status**: NEW (same class as closed #3746: 73 `_tmp_*` examples, 6 978 LOC, linked twice per CI run)
- **Age**: 2026-10-13 to 10-14 (`git diff --diff-filter=A 5d31fd637 HEAD -- '*/examples/*.rs'`)
- **Effort**: small
- **Description**: 168 tracked examples, 22 888 LOC. None of the new ones is disposable by the guard's definition (each has a module doc and a real purpose: censuses, graph dumps, per-record probes), and none carries a `#[test]` or an assertion (44 of 44 score 0 on `grep -c '#\[test\]\|assert'`), so no coverage is lost by consolidating. Examples are compiled by `cargo test --workspace` (and linked against their crate's full dependency set), which was the driver for #3746. Many are 13-35 line variations on "open an archive, print one thing" (`hkx/examples/{class_census,start_clip_ba2,clip_info,skeleton_bones,skeletons_dump,dump_objects}.rs`, `plugin/examples/{base_model,cell_refs_by_base,sound_names,script_cells}.rs`).
- **Evidence**: `git diff --name-only --diff-filter=A 5d31fd637 HEAD -- '*/examples/*.rs' | xargs wc -l` = 2 680.
- **Impact**: CI wall time and link memory, not correctness; plus the next "which census do I run" question.
- **Related**: #3746, `no_tmp_scratch_examples_are_committed`.
- **Suggested Fix**: fold the one-liners per crate into a single `examples/inspect.rs` with subcommands (the hkx archive plumbing already repeats in `*_ba2.rs`); keep the multi-page censuses. Extend the guard with an example-count/LOC ceiling per crate if the pattern recurs.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files/docs
- [ ] **TESTS**: A regression test or guard pins this specific fix (or the existing guard is named)
