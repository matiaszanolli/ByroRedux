# ESM-D6-05: FO4/FO76 AVIF `ANAM` "Abbreviation" is an lstring but is read with `read_zstring`

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/6800
Labels: low, esm-plugin, bug, game:fo4, game:fo76
Source: docs/audits/AUDIT_ESM_2026-11-08.md

_Filed from `docs/audits/AUDIT_ESM_2026-11-08.md` (audit HEAD 3500d2737)._

- **Severity**: LOW (dead field, no consumer)
- **Dimension**: 6
- **Record/Sub-record**: AVIF `ANAM`
- **Location**: crates/plugin/src/esm/records/misc/effects.rs:63 (`b"ANAM" => out.abbreviation = read_zstring(&sub.data)`)
- **Status**: NEW
- **Description**: `ANAM` is `wbString` in TES4/FO3/FNV/Skyrim but `wbLString(ANAM, 'Abbreviation', 0, cpTranslate)` in FO4 (FO4.pas:11680) and FO76 (FO76.pas:12748 area). The parser is game-agnostic, so FO4/FO76 get a 4-byte id decoded as text.
- **Evidence**: `dump_record_subs Fallout4.esm AVIF Strength` -> `ANAM len=4 hex=[85 39 02 00]`, which `read_zstring` turns into `"\u{FFFD}9\u{2}"` (id 0x00023985). `grep abbreviation` finds no reader outside the parser and its tests.
- **Impact**: wrong data in `AvifRecord::abbreviation` on FO4/FO76; invisible today because nothing reads it.
- **Suggested fix**: `read_lstring_or_zstring` for `ANAM` (safe on Skyrim and older: they are non-localized or, for Skyrim, a localized plugin's `ANAM` string is not 4 bytes except for a 3-char abbreviation, where the same hazard as D6-01 applies, so gate on `GameKind` >= FO4 rather than switch blindly).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related decoders / other games' arms
- [ ] **LOCK_ORDER**: n/a unless a RwLock scope changes
- [ ] **TESTS**: A regression test pins this specific fix (real-data pin where an install exists)
