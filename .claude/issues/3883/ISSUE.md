# TD7-2026-09-05-05: `parse_weather_data` decodes the WTHR DATA payload with bare offsets while the named `SKYRIM_DATA_SIZE` sits six lines above it

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD7-2026-09-05-05), `/audit-tech-debt` full 9-dimension sweep at `fa5c4191`. Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW
- **Dimension**: 7 — Magic Numbers & Hardcoded Constants
- **Location**: `crates/plugin/src/esm/records/weather.rs::parse_weather_data` (constant: `SKYRIM_DATA_SIZE`, declared immediately above it)
- **Status**: NEW
- **Effort**: trivial (≤30 min)
- **Description**: `SKYRIM_DATA_SIZE: usize = 19` is declared directly above `parse_weather_data` and is used by the record dispatcher (`b"DATA" if sub.data.len() >= SKYRIM_DATA_SIZE`) and by a test fixture — but not by the decoder it documents. The decoder instead runs a ten-step ladder of bare length guards (`data.len() > 3`, `> 4`, `> 5`, `> 7`, `> 9`, `> 10`, `> 11`, `>= 15`, `> 16`, `> 18`) paired with bare indices (`data[3]` … `data[18]`), where the final `> 18` is the same gate as `>= SKYRIM_DATA_SIZE`.
- **Evidence**: the last guard, `if data.len() > 18 { record.wind_direction = data[17]; record.wind_direction_range = data[18]; }`, is exactly `len() >= SKYRIM_DATA_SIZE` spelled as a literal. The ladder also mixes `>` and `>=` conventions for the same predicate shape (`>= 15` for `data[14]`, `> 16` for `data[16]`) — every guard is arithmetically **correct**, verified index by index; the inconsistency is a readability cost, not a bug, and is why this stays LOW rather than being disproved outright.
- **Impact**: the byte layout of a shared FO3/FNV/Skyrim sub-record lives as twenty scattered integers, so a layout correction has to be applied in ten independent places with no compiler assistance and no shared name to grep for. The record has already had one layout correction (the function's own doc comment records *"byte 10 is thunder/lightning frequency and byte 11 is the classification bitmask (not byte 14)"*), which is evidence this layout does get revised.
- **Related**: #1631 / TD7-002 (CNTO sub-record size duplicated across two record parsers — same class, closed) · #2597 / FO4-D4-01 (bare `(130..=139)` band instead of named constants)
- **Suggested Fix**: replace the trailing `data.len() > 18` with `data.len() >= SKYRIM_DATA_SIZE`, and give each field a `const WTHR_<FIELD>_OFFSET: usize` (or a small `(offset, len)` table the ladder iterates) so the layout is stated once. **Explicitly not proposed** for the WATR `DNAM` decoder in `records/misc/water.rs`, which uses the same raw-offset style but annotates every offset against its xEdit definition inline — the named-constant fix there would add indirection without adding information.

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
- [ ] **DROP**: If Vulkan objects change, the Drop impl stays reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
