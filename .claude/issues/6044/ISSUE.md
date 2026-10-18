# ESM-2026-10-18-EXT-09: two `byroredux-plugin` guards are red at HEAD (user commit `317320437`)

Labels: low,esm-plugin,test-gap,bug
Source: docs/audits/AUDIT_ESM_2026-10-18.md

- **Severity**: LOW (test-gap; the decoders themselves are correct). Out of exterior scope, reported because it was found by the test baseline.
- **Dimension**: FormID & Load Order / Byte Accounting guards
- **Location**: `esm/records/misc/music.rs` (`parse_musc` takes no `remap` and has no exemption), `music.rs:46` and `misc/challenge.rs:107` (local `SubRecord` fixture builders)
- **Status**: NEW
- **Evidence**: `esm::records::tests::every_record_parser_takes_a_remap_or_is_explicitly_exempt` and `esm::records::test_support::tests::no_module_redeclares_a_local_subrecord_builder` fail. `parse_musc` reads only `FNAM`/`ANAM`, no FormID, so the fix is an `EXEMPT_NO_U32_READS` row plus `test_support::sub`.
- **Suggested Fix**: as above.

_Filed from `docs/audits/AUDIT_ESM_2026-10-18.md` (`/audit-suite --area exterior`, HEAD 6f44874ac). Fix with `/fix-issue <number>`._
