# TD9-2026-10-25-04: three assertion-free ignored "tests" are probes in test clothing

Labels: low, test-gap, tech-debt, bug
Issue: https://github.com/matiaszanolli/ByroRedux/issues/6402
(Immutable snapshot of the issue as filed; GitHub is authoritative for state.)

- **Dimension**: 9 (`test-gap`) - **Effort**: small
- **Location**: `byroredux/tests/skinning_e2e.rs:877` (`fnv_dump_global_skin_transform`), `:927` (`fnv_vertex_skin_dump_arms1`),
  `crates/nif/tests/sequence_names_census.rs:36` (`object_sequence_names`); also `skinning_e2e.rs:412` (`oblivion_vertex_world_check`,
  deliberately `eprintln!`-only with a stated reason, so not counted but the same shape)
- **What**: bodies print and `unwrap()` only; nothing can fail except a missing file. They are investigation dumps from M41 phase 1b
  and a name census, and `fnv_vertex_skin_dump_arms1`'s comment still reads as an open hunt ("rendering produces a long-ribbon vertex
  artifact"). Precedent: #5857 (VM timing census got a ceiling) and #6185 (skeever probe turned into a pinned test).
  The previous audit's assertion-free scan counted `unwrap()` as "has an assertion" and so never listed them.
- **Fix**: move the three to `examples/` (the example ceilings guard, `example_targets_hold_their_per_crate_ceilings`, applies), or pin one
  measured fact each; delete `fnv_vertex_skin_dump_arms1` if the ribbon artifact is long closed.

---
Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-25.md` (HEAD c7463c04d), finding `TD9-2026-10-25-04`. Fix with: `/fix-issue <this issue>`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / crates
- [ ] **TESTS**: A regression test or source-scan guard pins this specific fix (or the finding is doc-only and `_audit-validate.sh` / `cargo test` stays green)
