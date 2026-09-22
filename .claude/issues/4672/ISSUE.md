# PAR-D6-2026-09-21-03: BGSM/BGEM strings decode as strict UTF-8, so one non-UTF-8 byte in any path drops the whole material

Labels: low,bug,import-pipeline,game:fo4,game:fo76

## Description
`crates/bgsm/src/reader.rs:94-98`: `String::from_utf8(..)` -> `Error::InvalidString` fails the whole parse on one bad byte anywhere in any string field, and the material falls back to NIF defaults with a warning. The reference Material-Editor implementation reads with .NET `BinaryReader.ReadChars` under a replacement-fallback UTF-8 decoder (`BaseMaterialFile.cs:326-336`), which is lossy rather than strict. This repo's own BSA/BA2 name tables already decode lossily, so a lossy BGSM string could still match the archive key that named it.

Verified unchanged at HEAD `ee6d3fb39`: `reader.rs:94-98` still uses strict `String::from_utf8`.

## Evidence
Vanilla: 0 of 36,888 FO4/FO76 materials hit `InvalidString` (probe `bgsm-scan`) — mod-content only impact.

## Impact
Mod-content only. Every texture slot of an otherwise-valid material is lost over a single non-UTF-8 byte anywhere in its string fields.

## Related
PAR-D6-2026-09-21-02 (sibling I/O-discipline finding, same dimension)

## Suggested Fix
Decode with `from_utf8_lossy` (matching the reference implementation and this workspace's own archive readers) and warn once per file when replacement occurred.

Source: docs/audits/AUDIT_PARSERS_2026-09-21.md (PAR-D6-2026-09-21-03)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other archive/material/animation readers)
- [ ] **TESTS**: A regression test pins this specific fix