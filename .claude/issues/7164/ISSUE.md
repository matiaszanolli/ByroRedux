# #7164 — PAR-D4-2026-11-21-01: `the_provider_is_rebuilt_only_after_an_archive_replacement` is flaky

Labels: low,import-pipeline,bug,test-gap

- **Severity**: LOW (a default-lane test that fails at random)
- **Dimension**: Corpus Gates
- **Location**: `byroredux/src/texture_streamer.rs` (the test, ~l.440); `refresh_provider` reads the process-global `archive_replacements()`
- **Status**: NEW (introduced by #7144)
- **Description**: `archive::tests::open_shared_rereads_*` bumps `ARCHIVE_REPLACEMENTS` from a parallel test thread. A bump between `TextureStreamer::new` and the first `refresh_provider`, or before the last assert, reddens the test.
- **Evidence**: 2 failures in 200 runs of `the_provider_is_rebuilt open_shared` (one panic at l.444, one at l.456). The race was confirmed by reading.
- **Suggested Fix**: let `refresh_provider` take the current generation as an argument; the frame loop passes `archive_replacements()` and the test passes literals.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (the other codec / archive format / list walker)
- [ ] **TESTS**: A regression test pins this specific fix

Source: `docs/audits/AUDIT_PARSERS_2026-11-21.md`
