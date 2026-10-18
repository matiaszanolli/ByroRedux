# ESM-2026-10-18-EXT-07: FO3/FNV `FNAM` day/night fog power is authored but not decoded

Labels: low,esm-plugin,terrain-exterior,bug,game:fo3,game:fnv
Source: docs/audits/AUDIT_ESM_2026-10-18.md

- **Severity**: LOW. `fog_power` is documented as currently unconsumed.
- **Dimension**: Sub-Record Byte Accounting
- **Record / Sub-record**: `WTHR` / `FNAM`
- **Location**: `weather.rs:562-571` gates power/max on FO4/FO76/Starfield only; comment at `:546-548`
- **Status**: NEW (the #5001 sibling note was never filed)
- **Description**: xEdit gives FO3/FNV 24-byte `FNAM` = near/far/near/far/**day power**/**night power** (`wbWeatherFogDistance`, `wbGameMode > gmTES4R`). The code leaves both at 1.0.
- **Evidence**: FNV 63 of 63 and FO3 27 of 27 records are 24 B; only 10 FNV and 8 FO3 records have `(1.0, 1.0)`. The rest author 0.25 to 0.75.
- **Suggested Fix**: read the two floats for `Fallout3NV` when `len >= 24`.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix


_Filed from `docs/audits/AUDIT_ESM_2026-10-18.md` (`/audit-suite --area exterior`, HEAD 6f44874ac). Fix with `/fix-issue <number>`._
