# ESM-2026-09-21-D4-01: TRDA's payload is documented in xEdit, but #4068's fix declares "no cited source" and leaves FO4/FO76 response numbers and emotion keywords undecoded

**Issue**: #4645
**Filed**: 2026-09-22 (audit-publish, AUDIT_ESM_2026-09-21.md)

**Severity**: LOW
**Dimension**: Record Schema Dispatch & Coverage
**Location**: `crates/plugin/src/esm/records/misc/dialogue.rs:234-253`

## Description
The `TRDA` arm only finalizes/starts segments; its comment claims no cited xEdit/UESP source, but xEdit documents FO4 (`wbDefinitionsFO4.pas:9732-9740`, 20 bytes: Emotion KYWD@0, Response u8@4, Sound FormID@5, ...), FO76 (`wbDefinitionsFO76.pas:12045`, same + form-version gates), and SF1 (`wbDefinitionsSF1.pas:12815`, 12 bytes: Emotion KYWD, WEM u32, Emotion Out f32).

## Evidence
`crates/scripting/src/dialogue.rs:343-344` copies `emotion_type`/`response_number` into every `DialogueLine`; both stay 0 for all FO4/FO76 segments (default `ResponseSegment`).

## Impact
The per-segment response number (voice-file key `<INFO>_<n>`) is unrecoverable downstream on FO4/FO76. From FO4 on, emotion is a keyword FormID needing `remap_fid`, not TES5's u32 enum.

## Suggested Fix
Decode FO4/FO76 response number (@4) and remapped emotion KYWD; decode SF1 emotion KYWD + WEM id. Correct the comment and cite the xEdit ranges.

## Related
#4068 (closed — segment-split half only), #1311

## Source
docs/audits/AUDIT_ESM_2026-09-21.md (ESM-2026-09-21-D4-01)
