# OB-D6-01: nif_stats.rs --tsv histogram does not implement the byte-for-byte parity with tests/common::PerBlockHistogram its own module doc claims

**Issue**: #4265 — https://github.com/matiaszanolli/ByroRedux/issues/4265
**Labels**: low,nif-parser,nif,test-gap,game:oblivion,legacy-compat,bug

**Severity**: LOW
**Dimension**: Dimension 6 — Real-Data Validation
**Location**: `crates/nif/examples/nif_stats.rs (module doc + block_histogram keying); crates/nif/tests/common/mod.rs:826-874 (PerBlockHistogram, wire-name keying)`
**Status**: NEW

## Description
`crates/nif/examples/nif_stats.rs`'s module doc claims its `--tsv` histogram "mirrors byte-for-byte" `tests/common::PerBlockHistogram` (#1883 / NIF-D3-001). In fact `nif_stats.rs`'s `block_histogram` keys on `block.block_type_name()` (the parser's resolved/dispatched type, which collapses alias families like `NiTriShape`/`NiTriStrips`/`BSSegmentedTriShape` into one bucket), while `PerBlockHistogram` keys on the header-advertised wire name (the #3326 fix) — the two are not the same keying scheme.

## Evidence
`crates/nif/tests/common/mod.rs:865-874` explicitly reads `header.wire_name` per block for its histogram key; `crates/nif/examples/nif_stats.rs:211-212` keys on `block.block_type_name().to_string()` instead. A manual cross-check against a checked-in baseline TSV using this tool produces a wall of false "differences" that look like a severe regression but aren't.

## Impact
The actual regression-gate tests (which use the correctly-keyed `tests/common` module) are unaffected. The impact is purely on manual/ad hoc use of `nif_stats --tsv` for cross-checking corpus histograms: a developer using this documented workflow gets a misleading result and could waste time chasing a phantom regression, or worse, dismiss a real one as "just the known keying difference."

## Related
None filed. Adjacent to #3326 (the wire-name-keying fix `PerBlockHistogram` has and this tool lacks) and #2347 (comment noting the two implementations are maintained in parallel).

## Suggested Fix
Either implement the same wire-name keying in `nif_stats.rs`'s `--tsv` histogram as `PerBlockHistogram` uses, or correct the module doc to state the actual (block_type_name-based) keying and stop claiming byte-for-byte parity.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed by audit-publish from docs/audits/AUDIT_OBLIVION_2026-09-11.md — findings verified against live code during this publish run.*
