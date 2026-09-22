# PAR-D3-2026-09-21-01: BGSM/BGEM accept any version with the newest layout and never check for unconsumed bytes, so layout drift is silent

Labels: low,bug,import-pipeline,game:fo4,game:fo76

## Description
`crates/bgsm/src/base.rs:170-171`, `crates/bgsm/src/bgsm.rs:158-335`, `crates/bgsm/src/bgem.rs:96-190`: there is no version ceiling anywhere in `parse_after_magic`/`BgsmFile::parse`/`BgemFile::parse`, and neither `parse` function checks `Reader::remaining()` before returning `Ok`. An unknown future layout, or a version-gating bug in this crate, decodes to wrong field values with no signal at all. The reference implementation (Material-Editor `BaseMaterialFile.cs:179-234`) also has no version ceiling, so a warning rather than a hard `Err` matches the format's own precedent.

Verified unchanged at HEAD `ee6d3fb39`: neither `parse` function reads `remaining()`, and there is no `version > N` rejection anywhere in the three cited files.

## Evidence
Probe `bgsm-scan`:
- Vanilla uses only v2 (FO4: 6,616 BGSM + 283 BGEM) and v22 (FO76: 25,888 + 4,101).
- 0 of 36,888 vanilla files leave a byte unconsumed; the probe re-parsed each file with its last byte removed to confirm the check would be zero-noise.

## Impact
Silent wrong material fields on mod or future (post-v22) content, with no diagnostic distinguishing "parsed correctly" from "parsed against the wrong layout".

## Related
PAR-D4-2026-09-21-03 (the sibling finding that notes FO76 BGSM has no dedicated sweep)

## Suggested Fix
After parse, `warn!` (with the path) when `remaining() > 0` or `version > 22`. Add both conditions to the FO4/FO76 sweep.

Source: docs/audits/AUDIT_PARSERS_2026-09-21.md (PAR-D3-2026-09-21-01)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other archive/material/animation readers)
- [ ] **TESTS**: A regression test pins this specific fix