# #5301: ESM-2026-10-05-D5-02: Starfield WTHR fog distances are lifted even when the record authored no FNAM — the decoder's 10,000-unit default would become 700,000

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/5301
- **Labels**: low,esm-plugin,bug,terrain-exterior,game:starfield
- **Source**: `docs/audits/AUDIT_ESM_2026-10-05.md` (ESM-2026-10-05-D5-02)

_From `docs/audits/AUDIT_ESM_2026-10-05.md` (2026-10-05 comprehensive audit suite, HEAD a2c24b16e)._

- **Severity**: LOW. There is no vanilla exposure.
- **Dimension**: CELL / WRLD Walkers & Placement Data (Starfield unit lift)
- **Record / Sub-record**: `WTHR` / `FNAM`
- **Location**: `crates/plugin/src/esm/records/spatial_units.rs:168-171` (#5134 block); defaults at `crates/plugin/src/esm/records/weather.rs:353-356` and `:516-519`
- **Status**: NEW. It is a residual of the closed #5134.
- **Description**: `normalize` multiplies `fog_day_near/far` and `fog_night_near/far` by 70 unconditionally. `parse_wthr` leaves `fog_day_far` / `fog_night_far` at the engine-unit default `10000.0` when FNAM is absent or short. The WATR lane (#5151, `raw_dnam.len() >= offset + 4`) and every `Option` field gate on "authored", but WTHR does not, contradicting the module's own rule that a decoder default for an unauthored field is never lifted. `fog_height` is an `Option` and is only lifted when decoded, so it is fine.
- **Evidence**: `Starfield.esm` has 3 WTHRs, all with 72-byte FNAM; `ShatteredSpace.esm` has 0 WTHRs (`scripts/sf_wthr.py`). The existing tests (`starfield_public_index_lifts_wthr_fog_distances`) only cover an authored FNAM.
- **Impact**: A mod-authored FNAM-less Starfield WTHR would reach `translate_weather` with a 700,000-unit fog far, which effectively means no fog.
- **Related**: #5134, #5001 (closed), D5-01.
- **Suggested Fix**: Record an `fnam_authored` flag (or the raw FNAM length) on `WthrRecord` and gate the four distance lifts on it, as `watr_absorption_per_metre` does. Add a no-FNAM Starfield test.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
