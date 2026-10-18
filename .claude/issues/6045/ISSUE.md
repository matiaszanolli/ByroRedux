# ESM-2026-10-18-EXT-10: Oblivion WATR `SNAM` is a FormID but is read as a zero-terminated string

Labels: low,esm-plugin,bug,game:oblivion
Source: docs/audits/AUDIT_ESM_2026-10-18.md

- **Severity**: LOW. The field has no consumer.
- **Dimension**: Sub-Record Byte Accounting
- **Record / Sub-record**: `WATR` / `SNAM` (Oblivion)
- **Location**: `records/misc/water.rs`, `parse_watr` `b"SNAM" if Oblivion` arm (`surface_sound = read_zstring(..)`)
- **Status**: NEW
- **Description**: `wbDefinitionsTES4.pas` defines `SNAM` as `wbFormIDCk(SNAM, 'Sound', [SOUN])`. All 18 Oblivion records author 4 bytes, so the string is raw FormID bytes and is never remapped. FO3+ `SNAM` is not decoded at all.
- **Suggested Fix**: decode as a remapped `u32` (`sound_form`).

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix


_Filed from `docs/audits/AUDIT_ESM_2026-10-18.md` (`/audit-suite --area exterior`, HEAD 6f44874ac). Fix with `/fix-issue <number>`._
