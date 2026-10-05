# #5330: TD8-2026-10-05-03: Six committed examples open their module doc with "Scratch:", which the #5114 disposable-example guard does not match; two arrived this window

Labels: low,tech-debt,bug,test-gap
Filed from: docs/audits/AUDIT_TECH_DEBT_2026-10-05.md

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-10-05.md` (TD8-2026-10-05-03) · audit HEAD `a2c24b16e`

- **Severity**: LOW
- **Dimension**: 8 — Dead Code & Backwards-Compat Cruft
- **Location**:
  - `crates/nif/examples/ragdoll_dump.rs:1` (new, `5ae7f8ad4` 10-04)
  - `crates/plugin/examples/xclw_census.rs:8` ("Scratch probe for the W2 LOD-coverage investigation — not a
    gate", new, `98061ec58` 10-03)
  - `crates/nif/examples/texset_dump.rs:1` (09-28)
  - `crates/nif/examples/dump_nolighting.rs:1`, `dump_alpha.rs:1`, `import_probe.rs:1` (May)
  - The guard is `byroredux/src/workspace_hygiene_tests.rs:56-59`
- **Status**: NEW (a gap in CLOSED #5114's guard)
- **Effort**: trivial
- **Description**:
  - `no_committed_example_self_describes_as_disposable` matches a doc line that starts with "throwaway",
    "one-off" or "temp scratch", or that contains "not for commit".
  - "Scratch:" is the codebase's most common self-description of a disposable probe, and it passes.
  - `AUDIT_EXTERIOR_2026-10-05` already found a bug in one of these probes (`xclw_census.rs:89-91`, a missing
    `.abs()`) whose output #5244 cites as reconciliation evidence. So a "scratch" probe is load-bearing evidence
    the moment an issue quotes it.
- **Suggested Fix**:
  - Add `"scratch"` to `LINE_START_MARKERS`.
  - Then either delete each flagged probe or rewrite its doc to state what it measures and why it stays, as in
    `watr_wind_census.rs`.
- **Related**: #5114 (CLOSED), TD8-2026-09-29-01; EXTERIOR-2026-10-05 (the `xclw_census` ring bug).

---

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix
