# PAR-D2-2026-09-21-04: A stale duplicate test pins "current" LZ4 under-run behaviour on a false premise

Labels: low,bug,import-pipeline,tech-debt,test-gap

## Description
`crates/bsa/src/ba2.rs:1734-1770`: `decompress_chunk_lz4_undersized_declared_size_currently_truncates_silently` has the same body as `decompress_chunk_lz4_under_run_returns_actual_length_not_declared` (`:1570-1587`). Its doc comment says "#2618 (MEDIUM, open ...) ... Once #2618 lands, this assertion should flip." #2618 is CLOSED (verified via `gh issue view`) and nothing has flipped.

The comment also calls `min_uncompressed_size` "only a capacity hint (`Vec::with_capacity`)". #3392 and the LZ4 arm's own current comment (`:817-823`) correct that: under the `safe-decode` feature this workspace pins, it is a hard output bound, not merely a hint.

Verified unchanged at HEAD `ee6d3fb39`: both the duplicate test and its stale doc comment are present verbatim; `gh issue view 2618` confirms CLOSED state.

## Evidence
See the cited location — the test body is a byte-for-byte duplicate of the sibling test, and #2618's closed state is confirmed against the live tracker.

## Impact
Misleading documentation about memory-safety-relevant decoder behaviour, plus a redundant test that pins nothing new.

## Related
#2618 (closed — the event this test's comment says hasn't happened yet), #2630, #3392

## Suggested Fix
Delete the duplicate test, or fold its intent into the surviving test's doc and correct the `min_uncompressed_size` description to match #3392's finding.

Source: docs/audits/AUDIT_PARSERS_2026-09-21.md (PAR-D2-2026-09-21-04)

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix