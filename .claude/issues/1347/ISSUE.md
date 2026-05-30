# #1347 — D2-02: ESM read_sub_records does not handle the XXXX extended-size override

_Snapshot as filed from AUDIT_FNV_2026-05-30 (d2-02). GitHub is authoritative for live state — query `gh issue view 1347 --json state`._

**Severity**: LOW · **Dimension**: ESM Record Parser · **Source**: AUDIT_FNV_2026-05-30 (D2-02)

> Label note: repo has no `esm` domain label; filed under `import-pipeline`.

**Location**: `crates/plugin/src/esm/reader.rs` (`read_sub_records`, fn at ~472)

**Description**: Sub-records larger than 0xFFFF use an `XXXX` sub-record carrying the true u32 size of the following field. `read_sub_records` doesn't special-case it. FalloutNV.esm has exactly one (a 177 KB WRLD `OFST`); benign today because no parser reads OFST and outer-stream alignment is force-restored, so no count/data regression. Real format gap that would bite a typed parser needing a sub-record positioned after an XXXX-prefixed oversized one (higher exposure on FO4/Skyrim).

**Evidence**: `read_sub_records` (reader.rs:472) loops on the 16-bit sub-record length with no `XXXX`/extended-size branch. (The `0xFFFFFF` at reader.rs:252 is unrelated form-ID masking.)

**Impact**: None today on FNV. A latent format gap for any future typed parser reading a sub-record after an XXXX-prefixed oversized field.

**Suggested Fix**: Handle `XXXX` in `read_sub_records` — when seen, read its u32 payload as the override size of the *next* sub-record instead of using that record's own 16-bit length field.

## Completeness Checks
- [ ] **SIBLING**: Confirm the typed record parsers that iterate sub-records (WRLD, CELL, large LAND/NAVM) tolerate or consume the XXXX override once added.
- [ ] **TESTS**: Add a synthetic record with an XXXX-prefixed >64 KB sub-record followed by another sub-record; assert correct positioning.
