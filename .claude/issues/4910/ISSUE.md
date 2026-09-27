# EXT-D5-2026-09-27-02: #4727's +90° "compass bearing" conversion applies to every game — the census supports only Skyrim/FO4, and Oblivion's layer 0 is a Cartesian vector

**Issue**: #4910
**Filed**: 2026-09-27 (audit-publish, AUDIT_EXTERIOR_2026-09-27.md)
**Labels**: medium,terrain-exterior,bug,water,esm-plugin

**Severity**: MEDIUM. The correct frame for FO3/FNV/Starfield is undetermined rather than proven wrong. For Oblivion the rotation is unsupported by any evidence.
**Dimension**: Water translation (WATAL)
**Tier Violated**: no-fabrication
**Game Affected**: Oblivion, FO3, FNV, FO76, Starfield
**Status**: NEW (scope gap in #4727)
**Location**:
- `byroredux/src/env_translate.rs:539-556`, applied at `:866` and `:931`.
- Inputs in `crates/plugin/src/esm/records/misc/water.rs`: Oblivion `:559-565` (`y.atan2(x)` of DATA 28/32), FO3/FNV `:759-763`, FO76/Starfield `:1285-1289`.
**Source**: `docs/audits/AUDIT_EXTERIOR_2026-09-27.md` (HEAD `0e0d35b96`)

## Description
- **Oblivion.** Layer 0 is the angle of the editor's (x, y) scroll pair, not a bearing, so the output changed from (x, y) to (−y, x). This affects e.g. `DefaultWater`.
- **FO3/FNV.** 71/78 FNV and 47/53 FO3 records carry non-zero layer speeds, and none has a NAM0 to census against. FNV `CreekWater01` layer 0 (0.228 UV/s) now runs along +Z instead of +X.
- **The re-run census:**

  | Game | Layers | Mean offset after conversion | R |
  |---|---|---|---|
  | Skyrim | 111 | +6.2° | 0.74 |
  | FO4 | 107 | +1.5° | 0.65 |
  | FO76 | 138 | +12.5° | 0.36 (weak) |
  | Starfield | 36 | — | 0.21 (no support either way) |

- Neither the code doc nor watal.md scopes the conversion by game.

## Impact
Authored layer motion on Oblivion, FO3, FNV and Starfield changed direction on 2026-09-24 without evidence. FNV is the reference title.

## Suggested Fix
- Convert per layout at the parse boundary: Skyrim and FO4, FO76 tentatively. Leave Oblivion's Cartesian pair unrotated.
- Record FO3/FNV and Starfield as open in watal.md §2.
- Add per-game pins.

## Related
#4727, EXT-D5-01, #3144

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other spawners / translate arms / games)
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the EXAL/WATAL translate boundary (`env_translate.rs`, `groundcover_translate.rs`, the `cell_loader` spawn sites) — never pushed into shaders/renderer, never re-derived at render time. See `/audit-exterior`.
- [ ] **TESTS**: A regression test pins this specific fix
