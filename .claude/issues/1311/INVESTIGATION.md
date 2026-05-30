# Investigation — #1311 (OBL-D3-01) — ALREADY FIXED (duplicate of #1304 body)

## Conclusion
No code change required. #1311 (`OBL-D3-2026-05-28-01`) is the **same finding** that was
fixed earlier this session in commit **714f398c** ("Fix #1304: INFO TRDT byte 0 is
EmotionType, not a response number"). #1304 had a title/body mismatch — its *body* was
this exact OBL-D3 TRDT finding — so fixing #1304's body resolved #1311 too.

## Premise is now stale (verified against current code)
#1311 claims `InfoRecord.response_type` captures `sub.data[0]`. That field no longer
exists. Current `crates/plugin/src/esm/records/misc/ai.rs`:
- `:315 pub emotion_type: u8` (renamed; doc at :314 notes "was mislabeled `response_type`")
- `:320 pub response_number: u8`
- `:367 out.emotion_type = sub.data[0]`
- `:368-369 if sub.data.len() >= 13 { out.response_number = sub.data[12] }`
And the unit test at `tests.rs:177-178` now asserts `emotion_type == 3` (EMO_Fear) +
`response_number == 5` on a full 16-byte TRDT fixture.

## #1311's suggested fix — all already done by 714f398c
- rename → `emotion_type: u8` ✓
- capture `response_number = sub.data[12]` when `len >= 13` ✓
- doc comment corrected ✓ (cites the TES4 TRDT layout — note: the issue's suggested
  "OpenMW `TargetResponseData`" citation doesn't resolve; openmw has no TES4 INFO
  parser, so 714f398c cited the empirical 23,877-TRDT histogram + the standard TES4
  layout instead)
- unit test fixed ✓
- **SIBLING (FO3/FNV)**: `parse_info` is the single shared INFO parser and the TES4
  TRDT first-16-byte layout (EmotionType u32 @0, Response number u8 @12) is identical
  across Oblivion/FO3/FNV, so the fix already applies to all three.

## Action
Closed as a duplicate of #1304 (fixed in 714f398c). No new commit.
