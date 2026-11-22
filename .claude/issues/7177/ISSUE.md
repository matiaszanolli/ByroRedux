# #7177 — PAR-D2-2026-11-21b-08: surplus data after a Bink Audio packet's blocks is accepted silently (PLAUSIBLE)

Labels: low,import-pipeline,bug

- **Severity**: LOW
- **Dimension**: Error Semantics
- **Location**: `crates/binkaudio/src/decoder.rs` `Decoder::decode`
- **Status**: NEW (the other side of #7163's check)
- **Description**: the overrun check refuses data that is too short. Data longer than the blocks it states is never compared with where decoding ended. In the fixture probe, appending 64 junk bytes to every packet decoded `Ok` with identical output (5 of 5). This only matters for a degraded soundtrack the sweep would not flag; it is not a safety issue.
- **Suggested Fix**: measure as A10 did (any installed packet whose aligned end is not `8 * len`?). If none, refuse or warn on surplus.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (the other codec / archive format / cache key)
- [ ] **TESTS**: A regression test pins this specific fix

Source: `docs/audits/AUDIT_PARSERS_2026-11-21b.md`
