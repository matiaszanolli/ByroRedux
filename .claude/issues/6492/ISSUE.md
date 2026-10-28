# REG-2026-10-27-05: Regression of #6193: the non-blocking `cargo clippy --all-targets` step is red again

Labels: low, tech-debt, bug
Source: docs/audits/AUDIT_REGRESSION_2026-10-27.md

**Audit report**: `docs/audits/AUDIT_REGRESSION_2026-10-27.md` -- finding `REG-2026-10-27-05` (LOW)
**Location (re-mapped to current code)**: `crates/plugin/examples/shore_probe.rs` (`const CELL` at :28 unused -> dead_code; type_complexity on the `found: Vec<(f32, Vertex, (i32,i32), f32, f32, usize)>` at ~:89; from `ceaf85a54`); `byroredux/src/cell_loader/spawn/synthesize_trimesh_tests.rs` (~:599-608, 3x `field_reassign_with_default`, from `1efe39722`); `byroredux/src/scene/world_setup.rs` (~:2218-2219, `field_reassign_with_default` in #6406's test, `8e20ced6b`). Step: `.github/workflows/ci.yml` "cargo clippy (--all-targets, non-blocking)". Premise re-read at HEAD: `CELL` is declared and never used; the reassign patterns are still present.
**Status at filing**: Regression of #6193 (CLOSED 10-22; itself a regression of #5850). No open issue tracks it.

## Description
The non-blocking `--all-targets` clippy step is red again. Because it sets `continue-on-error: true`, GitHub reports it as `success`; its log on `eae8fd740` ends with `could not compile byroredux-plugin (example "shore_probe") due to 2 previous errors`, `could not compile byroredux (bin "byroredux" test) due to 4 previous errors`, exit 101.

## Evidence
Job log of CI run 40596734340, step "cargo clippy (--all-targets, non-blocking)". Sites: `shore_probe.rs` dead_code `CELL` (:28) and `type_complexity` (:89); 3x `field_reassign_with_default` in `synthesize_trimesh_tests.rs` (:599-608); `field_reassign_with_default` in `world_setup.rs` (:2218-2219).

## Impact
Lints accumulate in test and example targets without anyone seeing them. Third time this step has gone red after a cleanup.

## Related
#6193, #5850, #5621, #6222.

## Suggested Fix
Apply the six mechanical fixes. Making the step's failure visible (e.g. a job summary line) would stop the silent green.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)

