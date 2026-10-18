# ESM-2026-10-18-EXT-06: Oblivion cloud decode: `DNAM`/`CNAM` layer order is the reverse of the record's own names, and the authored cloud speeds in `DATA[1..2]` are dropped

Labels: low,esm-plugin,terrain-exterior,bug,game:oblivion
Source: docs/audits/AUDIT_ESM_2026-10-18.md

- **Severity**: LOW. Visual fidelity only; the sky falls back to the synthetic wind-driven drift.
- **Dimension**: Sub-Record Byte Accounting
- **Record / Sub-record**: `WTHR` / `CNAM`, `DNAM`, `DATA`
- **Location**: `weather.rs:684-685` (`DNAM`→layer 0, `CNAM`→layer 1), `:771` (the #4015 note on `DATA` bytes 1 to 2)
- **Status**: NEW
- **Description**: `wbDefinitionsTES4.pas:3585-3587` and the local UESP page define `CNAM` = Lower Cloud Layer and `DNAM` = Upper Cloud Layer, and `DATA[1]`/`DATA[2]` = Cloud Speed (Lower)/(Upper). The #4015 comment ("no source identifies bytes 1 to 2") is true for FO3/FNV, where xEdit marks them `wbUnused(2)`, but not for Oblivion. I could not verify which layer index the renderer treats as "lowest", so the order finding is partly unverified.
- **Evidence**: all 37 Oblivion weathers author non-zero speeds in those bytes (for example 42/19, 50/25, 101/63, 255/134), while `cloud_layer_velocities_authored` stays `false` for them.
- **Suggested Fix**: for `GameKind::Oblivion` map `CNAM`→0, `DNAM`→1 once the layer-order semantics are confirmed, and read `DATA[1..2]` into `cloud_layer_velocities` with `authored = true`.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix


_Filed from `docs/audits/AUDIT_ESM_2026-10-18.md` (`/audit-suite --area exterior`, HEAD 6f44874ac). Fix with `/fix-issue <number>`._
