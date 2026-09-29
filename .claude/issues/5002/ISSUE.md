# SF-2026-09-29-D4-03: parse_lgtm reads Starfield's 108-byte LGTM DATA with Skyrim tail offsets; normalize then lifts two dimensionless scales by 70 (dormant)

**Labels**: low,bug,esm-plugin,game:starfield,legacy-compat

**Source**: `docs/audits/AUDIT_STARFIELD_2026-09-29.md`
**Severity**: LOW (dormant on vanilla)
**Dimension**: ESM → cell bring-up
**Location**: `crates/plugin/src/esm/records/misc/world.rs` (`parse_lgtm`, `>= 92` arm); `crates/plugin/src/esm/records/spatial_units.rs` (`normalize`, lighting_templates loop); `byroredux/src/cell_loader/load.rs` (comment "SF volumetric height-fog fields ride on inline XCLL rather than Skyrim-style LGTM templates")

## Description
`parse_lgtm` has no game parameter. For DATA ≥ 92 bytes it skips 32 bytes from offset 40 and reads `fog_far_color`@72, `fog_max`@76, `light_fade_begin`@80 and `light_fade_end`@84 — Skyrim's layout. xEdit SF1 `LGTM.DATA` (`wbDefinitionsSF1.pas:13758-13790`) is the SF XCLL shape: Fog Color Far@40, Fog Max@44, Light Fade Start/Stop@48/52, then Height Mid/Range@60/64, High colours@68/72, High Density Scale@76, Fog Near/Far Scale@80/84, …. The four Starfield fields therefore come from High Far colour, High Density Scale, Fog Near Scale and Fog Far Scale. `spatial_units::normalize` then multiplies the two scales (read as light fades) by 70. The height-fog block is dropped. The `load.rs` comment contradicts xEdit.

## Evidence
All 6 `Starfield.esm` LGTMs carry a 108-byte DATA. `ShipInteriorLT` 0x6658 has authored Fog Max@44 = 0 and fades@48/52 = 163,840/163,840. The parser yields `fog_max` = 1.0 (@76), `light_fade_begin/end` = 1.0/1.0 (@80/84), lifted to 70/70 BU.

## Impact
Dormant on vanilla: `resolve_cell_lighting` consults an LGTM only when XCLL is absent or its inherit mask is set, and every Starfield interior CELL carries a 108-byte XCLL with no inherit mask. A plugin CELL with LTMP and no XCLL would get fog max 1.0 and light fade 70 BU (lights culled beyond 1 m).

## Related
SF-2026-09-29-D4-01 / -D4-02 (same Starfield-layout-at-the-boundary class); #1579 (the XCLL analogue, fixed).

## Suggested Fix
Pass `GameKind` into `parse_lgtm` and decode Starfield DATA with the same offset table as the SF XCLL arm (`crates/plugin/src/esm/cell/walkers.rs`). Populate the `starfield` height fog on the template, lift it in `normalize`, and correct the `load.rs` comment.

Validated at HEAD 9fcfdc3fc: `parse_lgtm(form_id, subs)` takes no game and its `>= 92` arm does `skip_or_eof(32)` then reads fog_far_color/fog_max/fades; `normalize` lifts `light_fade_begin/end` for Starfield; the `load.rs` comment is present.

## Completeness Checks
- [ ] **SIBLING**: other records decoded with Skyrim offsets on Starfield (XCLL arm is the reference)
- [ ] **TESTS**: A regression test pins this specific fix (108-byte Starfield LGTM fixture)
