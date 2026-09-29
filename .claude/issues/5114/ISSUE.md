# TD8-2026-09-29-01: Eleven self-described "Throwaway" probe examples stay committed; the `_tmp_` guard misses `tmp_`

**Labels**: low,tech-debt,bug

**Source report**: `docs/audits/AUDIT_TECH_DEBT_2026-09-29.md`

- **Severity**: LOW · **Dimension**: 8 · **Status**: NEW · **Effort**: small
- **Location**:
  - `crates/nif/examples/tmp_fo4_d4_{psglod,lodoverlap,lodsize}.rs` (`f74f8f68a`, 2026-09-12). Header:
    "Throwaway (FO4 audit D4): census …".
  - `crates/bsa/examples/{obl_sweep,probe_substring,probe_extensions}.rs`,
    `crates/plugin/examples/{find_ext_cell,roof_probe,qust_alias_rawdump}.rs`,
    `crates/nif/examples/lod_probe.rs` and `crates/bgsm/examples/dump_bgsm.rs`. These date from
    2026-05-05 → 07-21, and each module doc says "Throwaway" or "One-off diagnostic".
  - 669 LOC in total.
  - Guard: `byroredux/src/workspace_hygiene_tests.rs:26-29`.
- **Evidence**:
  - The guard matches only `starts_with("_tmp_")`.
  - NIFAL reports 09-14, 09-16 and 09-21 each routed the three `tmp_fo4_d4_*` files here. No tech-debt
    report picked them up.
  - `clippy --all-targets` already fails on all three.
  - Each one is an example target that links on every workspace test run, which is the cost #3746
    measured.
- **Related**: #3746, #3150
- **Suggested Fix**:
  - Delete the probes, or give a keeper a real documented purpose.
  - Widen the guard to `tmp_`/`_tmp_` plus a `^//! *(Throwaway|One-off|TEMP scratch)` module-doc check.

**Validated at HEAD 9fcfdc3fc**: all 11 listed example files exist (669 LOC total); `byroredux/src/workspace_hygiene_tests.rs` matches only `name.starts_with("_tmp_")`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix
