# ESM-D2-03: `GDRY.DATA` has no length gate and reads 15 fixed fields with `f32_or_default`; `parse_voli` / `parse_gdry` re-implement EDID

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/6783
Labels: low, esm-plugin, bug, game:fo4, game:fo76
Source: docs/audits/AUDIT_ESM_2026-11-08.md

_Filed from `docs/audits/AUDIT_ESM_2026-11-08.md` (audit HEAD 3500d2737)._

- **Severity**: LOW
- **Dimension**: Sub-Record Byte Accounting
- **Record / Sub-record**: `GDRY` / `DATA` (FO4, FO76); `VOLI` / `CNAM`..`NNAM`
- **Location**: `crates/plugin/src/esm/records/god_rays.rs:63-69` (`edid`), `:70-76` (`float_of`), `:94-110` (`parse_gdry`); the test `parse_gdry_decodes_the_60_byte_data` ends by asserting that a 28-byte `DATA` decodes the first fields and zeroes the rest
- **Status**: NEW
- **Description**: `GDRY.DATA` is a fixed 60-byte struct (3 floats + 3 floats + 4 + 1 + 3 + 1 floats, xEdit FO4.pas:16598-16611, FO76.pas:18083-18095). The decoder reads all 15 floats through `f32_or_default`, so any `DATA` shorter than 60 yields a plausible partial record with silent zeros mid-layout. This is the case the checklist classifies as a finding ("`*_or_default` ... a finding mid-fixed-layout"); `AOR2_LEN` in the same window declines a short body instead. `edid()` is a hand-rolled copy of `CommonNamedFields` EDID handling.
- **Evidence**: Fallout4.esm has 61 `GDRY`, `DATA` = 60 B on all 61; SeventySix.esm has none of this shape to contradict it. VOLI is 12 single-float sub-records (TES5.pas:14018-14032, all 4 B in Skyrim Update.esm and FO76: `CNAM` 4 B x 213), where a short sub-record yielding 0.0 is acceptable.
- **Impact**: none on vanilla data; wrong-but-plausible numbers on a modded short `DATA` once a consumer exists (PRD §10 Q8 says none does yet).
- **Suggested fix**: `GDRY_DATA_LEN = 60`; leave the record at defaults when `DATA` is shorter (same as `AOR2_LEN`). Take `editor_id` from `CommonNamedFields::from_subs_with_remap`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related decoders / other games' arms
- [ ] **LOCK_ORDER**: n/a unless a RwLock scope changes
- [ ] **TESTS**: A regression test pins this specific fix (real-data pin where an install exists)
