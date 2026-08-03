# OBL-D6-01: nif_stats --tsv header line drifted from the test-harness to_tsv (cosmetic)

**Issue**: https://github.com/matiaszanolli/ByroRedux/issues/2347
**Severity**: LOW
**Dimension**: Real-Data Validation
**Location**: `crates/nif/examples/nif_stats.rs:467-475` (`Stats::print_tsv`) vs `crates/nif/tests/common/mod.rs:579-592` (`PerBlockHistogram::to_tsv`)
**Source audit**: `docs/audits/AUDIT_OBLIVION_2026-08-03.md` (finding OBL-D6-01)
**Labels**: low, nif-parser, tech-debt, bug

### Description
`nif_stats --tsv`'s header line and the test-harness
`PerBlockHistogram::to_tsv` header have drifted apart — `nif_stats.rs`
includes `clean=`/`truncated=` fields the test-harness version doesn't. Both
implementations agree on every one of the 81 data rows; only the
`#`-prefixed header comment differs, which both parsers skip.

### Impact
Zero functional impact — maintenance/consistency gap only.

### Suggested Fix
Have `nif_stats.rs`'s `--tsv` path reuse `PerBlockHistogram::to_tsv` instead
of maintaining a second header format.
