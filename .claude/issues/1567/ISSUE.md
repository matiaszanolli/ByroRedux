**Severity**: HIGH · **Dimension**: SF ESM Resolve-Rate
**Location**: `crates/plugin/src/esm/cell/support.rs:23-160` (`build_static_object_from_subs`), dispatched from `crates/plugin/src/esm/records/mod.rs:297-300`
**Source**: `docs/audits/AUDIT_STARFIELD_2026-06-14.md` (SF-D4-01)

## Description
`build_static_object_from_subs` builds `light_data` for `LIGH` records by reading a `DATA` subrecord at UESP-Skyrim offsets. Starfield LIGH records carry **no `DATA` and no `MODL`** — they use a component-block layout (`BFCB`…`BFCE` wrappers around `FLCS`/`FLTR`/`FLLD`/`DAT2`/`FLBD`/`FLRD`/`FLGD`/`LLLD`/`FLAD`/`FVLD`). With no `MODL` and no `DATA`-derived `light_data`, the function returns `None`, the LIGH form is never inserted into `cells.statics`, and every REFR pointing at it misses at `references.rs:362` and is silently skipped.

## Evidence
Live subrecord dump of unresolved Cydonia LIGH forms: `LIGH 000027BB subs: EDID OBND ODTY BFCB FLCS BFCE BFCB INTV FLTR BFCE FLLD DAT2(76) FLBD FLRD FLGD(88) LLLD FLAD FVLD` — no `DATA`, no `MODL` (identical skeleton on `00024F71`, `0003657A`). FormID→FourCC classifier over the unresolved set: **656 LIGH REFRs across 62 distinct forms**. Confirmed in code: `support.rs:43` decodes `DATA` only when `is_ligh && sub.data.len() >= 12`, and `:41` reads only a top-level `MODL`. Distinct from the NIF-embedded `NiPointLight` path (#721), which is unaffected.

## Impact
Cydonia's ESM-placed interior lighting (sconces / lamps / practical lights authored as LIGH REFRs) is entirely absent — only NIF-embedded lights + XCLL ambient survive, so the cell renders markedly under-lit. Largest *functional* (renderable) contributor to the 11.2% resolve gap and a direct blocker to "Cydonia interior looks right."

## Related
#721 (NIF-embedded lights); SF-D4-03 (same `BFCB` component-block root cause — share the walker).

## Suggested Fix
Add a `GameKind::Starfield`-gated LIGH decode that walks the `BFCB`/`BFCE` component blocks and extracts color/radius from `DAT2`/`FLGD`/`FLLD` (byte-audit against the Gibbed.Starfield LIGH component schema first — no guessing offsets), emitting `light_data` so the existing `references.rs:386-404` light-only spawn path lights the cell. Leave FO4/Skyrim DATA-layout LIGH untouched.

## Completeness Checks
- [ ] **SIBLING**: The `BFCB`/`BFCE` component-block walker is reusable for the model-less STAT/ACTI/ARMO forms in SF-D4-03 (don't fork two walkers)
- [ ] **TESTS**: A regression test pins a real Cydonia LIGH form decoding to non-`None` `light_data` with the expected color/radius
