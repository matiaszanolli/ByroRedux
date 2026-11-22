# #7159 — PAR-D2-2026-11-21-04: BSA and CSG payload reads still fail with a bare "failed to fill whole buffer"

Labels: low,import-pipeline,bug

- **Severity**: LOW
- **Dimension**: Error Semantics
- **Location**: `crates/bsa/src/archive/extract.rs` (the `read_exact_at(..)?` calls), `crates/bsa/src/csg.rs:341`
- **Status**: NEW (sibling of #7131, closed: BA2 only)
- **Trigger Input**: a truncated BSA or CSG, or a record whose offset is past EOF.
- **Description**: these errors name no entry, offset or length. A cosmetic leftover is that labels double up: the plugin prints "Compressed record X: ESM record X: …", and `or_warn` prints "'p' failed to extract: BA2 zlib chunk of 'p': …".
- **Suggested Fix**: reuse the `ChunkSite` context wrapper (keeping the error kind) at those reads, and drop the duplicated outer labels.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (the other codec / archive format / list walker)
- [ ] **TESTS**: A regression test pins this specific fix

Source: `docs/audits/AUDIT_PARSERS_2026-11-21.md`
