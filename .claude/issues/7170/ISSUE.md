# #7170 — PAR-D1-2026-11-21b-02: #7159 formats a `String` before every BA2 chunk read, not only on failure

Labels: low,import-pipeline,bug

- **Severity**: LOW (an allocation per DX10 chunk read, on the streaming path)
- **Dimension**: Size Discipline
- **Location**: `crates/bsa/src/ba2.rs` `read_chunk_payload` (`let chunk = site.chunk.map_or(String::new(), |index| format!(" chunk {index}"));`)
- **Status**: NEW (introduced by #7159; the commit says a successful read formats nothing)
- **Description**: the label suffix used to be built inside `map_err`. It is now built before the read. The BSA and CSG sites pass `format_args!` and allocate nothing on success.
- **Suggested Fix**: a `Display` adapter for the optional chunk index, passed through `format_args!`.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (the other codec / archive format / cache key)
- [ ] **TESTS**: A regression test pins this specific fix

Source: `docs/audits/AUDIT_PARSERS_2026-11-21b.md`
