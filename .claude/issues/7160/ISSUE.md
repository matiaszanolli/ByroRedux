# #7160 — PAR-D2-2026-11-21-05: a truncated Bink 1 header gives `probe` an opaque I/O error

Labels: low,import-pipeline,bug

- **Severity**: LOW
- **Dimension**: Error Semantics
- **Location**: `crates/video/src/lib.rs` `probe_unguarded` (two `read_exact` calls)
- **Status**: NEW
- **Description**: a 100-byte file claiming 0xFFFFFFFF frames or tracks returns `Unreadable("failed to fill whole buffer")`, while `Movie::open` describes the same file. Nothing is allocated (6.9 MB peak RSS).
- **Suggested Fix**: name the table that ran short (header, track table or frame index).

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (the other codec / archive format / list walker)
- [ ] **TESTS**: A regression test pins this specific fix

Source: `docs/audits/AUDIT_PARSERS_2026-11-21.md`
