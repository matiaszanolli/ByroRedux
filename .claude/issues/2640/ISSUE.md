# SF-D6-06: SF_WEAK_REF_GAP doc claims bsver 174 unobserved, falsified by 13 real files

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2640
**Finding ID**: SF-D6-06

**Severity**: LOW
**Dimension**: 6 (NIF Shader Blocks, BSVER 155+)
**Location**: `crates/nif/src/version.rs:420-436`
**Status**: NEW (correction to a doc claim, not a code defect)

## Description
`SF_WEAK_REF_GAP`'s doc claim that bsver 174 is unobserved is falsified by
13 real files, which also close the gate boundary at exactly 175.
`Starfield - MeshesPatch.ba2` contains 13 bsver-174 terrain files, all
parsing with 0 `NiUnknown` under the current `SF_WEAK_REF_GAP = 175` gate —
i.e. 174 carries `form_id` but no 2-byte gap. The current constant is
right; the doc understates the confidence available and could invite a
wrong future widening to 174.

## Evidence
13 bsver-174 files in `Starfield - MeshesPatch.ba2`, all parsing clean
under the current gate.

## Impact
None — the code is correct; only the doc's confidence level is wrong.

## Suggested Fix
Update the version table row for 174 to `yes / no / MeshesPatch.ba2 (13
files)`, note the boundary is pinned at 175.

## Completeness Checks
- [ ] **TESTS**: N/A — doc-only change; code is already correct
