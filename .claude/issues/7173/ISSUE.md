# #7173 — PAR-D2-2026-11-21b-02: a binkaudio `Decoder` keeps the refused block's overlap after `Error::Overrun`

Labels: low,import-pipeline,bug

- **Severity**: LOW (latent: the only caller ends the track on any error)
- **Dimension**: Error Semantics
- **Location**: `crates/binkaudio/src/decoder.rs` `Decoder::decode`, `block` (`self.tail = …` runs before `decode`'s check)
- **Status**: NEW (introduced by #7163; documented in its doc comment)
- **Description**: `out` is rolled back, but the overlap carried into the next block is the refused block's. `BadSize` leaves the state untouched, so the two errors leave the decoder in different states.
- **Suggested Fix**: snapshot `tail` before the loop and restore it on `Overrun`, or document the decoder as unusable after any `Err`.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (the other codec / archive format / cache key)
- [ ] **TESTS**: A regression test pins this specific fix

Source: `docs/audits/AUDIT_PARSERS_2026-11-21b.md`
