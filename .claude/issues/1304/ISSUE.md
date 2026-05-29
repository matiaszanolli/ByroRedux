# #1304 -- OBL-D3-01: INFO TRDT byte 0 is EmotionType not response_type

_Snapshot as filed 2026-05-28 from /audit-publish (AUDIT_OBLIVION_2026-05-28)._

**Severity**: MEDIUM | **Dim 3** — ESM Record Coverage
**Source**: `docs/audits/AUDIT_OBLIVION_2026-05-28.md` (OBL-D3-2026-05-28-01)

**Location**: `crates/plugin/src/esm/records/misc/ai.rs:300-304, 343-345`

**Issue**: `InfoRecord.response_type` captures `sub.data[0]` as the dialogue "Response Type" but that byte is actually the low byte of `EmotionType` (u32 @ TRDT offset 0: EMO_Neutral=0..EMO_Surprise=6). The real Response number lives at TRDT offset 12 and is never read, making it unrecoverable. Empirically verified on all 23,877 TRDT subrecords in `Oblivion.esm` (277 MB): byte[0] histogram `{0:9634,1:3288,2:1964,3:1568,4:1444,5:4475,6:1504}` = EmotionType (0–6 = Neutral..Surprise). The unit test at `tests.rs:100/168` also enshrines the wrong semantics (value 3 is EMO_Fear, not a response number).

**Suggested fix**: rename to `emotion_type: u8`; also capture `response_number = sub.data[12]` when `sub.data.len() >= 13`; update the doc comment to cite the OpenMW `TargetResponseData` layout; fix the unit test.

## Completeness Checks
- [ ] **SIBLING**: check FO3/FNV INFO TRDT layout for the same field offset (FO3+ may be identical)
- [ ] **TESTS**: unit test updated to assert the correct EmotionType semantics + response_number
- [ ] **CANONICAL-BOUNDARY**: ESM parse-side; no material/translate impact
- [ ] **UNSAFE**: no unsafe involved
