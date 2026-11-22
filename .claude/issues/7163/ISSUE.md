# #7163 — PAR-D2-2026-11-21-08: Bink Audio has no end-of-packet overrun check (PLAUSIBLE)

Labels: low,import-pipeline,bug

- **Severity**: LOW
- **Dimension**: Error Semantics
- **Location**: `crates/binkaudio/src/decoder.rs` `Decoder::decode`
- **Status**: NEW (sibling of #7127; not demonstrated)
- **Description**: the only guard is the lower bound `blocks * min_block_bits <= 8 * data.len()`. Nothing checks that the reader ends inside the packet, so a cut packet that still passes the bound may read missing bits as zeros. The truncation probe hit `BadSize` only.
- **Suggested Fix**: as #7127 did, measure first: run a census over the corpus for reads past `8 * len`. If none occur, refuse them with `Error::Overrun`.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (the other codec / archive format / list walker)
- [ ] **TESTS**: A regression test pins this specific fix

Source: `docs/audits/AUDIT_PARSERS_2026-11-21.md`
