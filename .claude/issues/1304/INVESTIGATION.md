# Investigation — #1304 INFO TRDT response_type → emotion_type

## ⚠️ Title/body mismatch (flag for user)
Issue #1304's **title** is "OBL-D4-NEW-01: Oblivion renders without normal maps"
(renderer, labels: renderer/high), but its **body** is a completely different
finding — **OBL-D3-2026-05-28-01**: `InfoRecord.response_type` misreads the TRDT
EmotionType byte (esm, medium). The body is the detailed, empirically-verified,
actionable content, so it is what was fixed. The normal-maps title is NOT
addressed by this change. Surfaced to the user before closing.

## Premise — CONFIRMED against current code
- `crates/plugin/src/esm/records/misc/ai.rs:354` reads `out.response_type = sub.data[0]`.
- Doc comment (307-311) labels byte 0 the "Response_Type enum".
- Test `crates/plugin/src/esm/records/tests.rs:100/168`: fixture `TRDT=[3,0,0,0]`,
  asserts `response_type == 3` — enshrining the wrong semantics (3 = EMO_Fear).

## Root cause
TES4 `TRDT` (INFO dialogue response data) layout:
`EmotionType(u32 @0) + EmotionValue(i32 @4) + unused[4] @8 + Response number(u8 @12) + unused[3]`.
Byte 0 is the low byte of the EmotionType enum (0=Neutral … 6=Surprise), NOT a
response number. The issue's empirical histogram over all 23,877 `Oblivion.esm`
TRDTs — `{0:9634,1:3288,2:1964,3:1568,4:1444,5:4475,6:1504}` — is exactly the 0–6
EmotionType distribution. The real response index (offset 12) was never read.

## Fix
- `InfoRecord.response_type: u8` → `emotion_type: u8` (corrected doc).
- Added `response_number: u8` = `sub.data[12]` when `len >= 13`.
- `parse_info` TRDT arm reads both; doc cites the TES4 layout.
- Test fixture extended to a full 16-byte TRDT (emotion=3 @0, response#=5 @12);
  asserts `emotion_type == 3` and `response_number == 5`.

## Completeness checks
- **SIBLING (FO3/FNV)**: `parse_info` is the single shared INFO parser; FO3/FNV
  use it and their TRDT first-16-byte layout is identical (EmotionType@0,
  Response#@12), so the fix applies to all three. ✓
- **TESTS**: updated to the correct EmotionType semantics + response_number. ✓
- **CANONICAL-BOUNDARY**: ESM parse-side only; no material/translate impact. ✓
- **UNSAFE**: none. ✓

## Notes
- Only `ai.rs` + `tests.rs` reference the field; rename is complete (workspace builds).
- The untracked `crates/plugin/examples/obl_info_probe.rs` (the diagnostic that
  produced the issue's histogram) now reports 0 INFO records on a fresh run — a
  bug in that throwaway tool, unrelated to this fix; not chased.
- Reference: TES4 INFO TRDT layout (UESP / xEdit). openmw has no TES4 INFO parser
  (Morrowind-only), so the issue's "OpenMW TargetResponseData" citation does not
  resolve; the empirical histogram + standard TES4 layout are the authority.

## Verification
446 plugin lib tests pass (incl. `dial_topic_children_walked_into_dialogue_infos`);
workspace builds clean.
