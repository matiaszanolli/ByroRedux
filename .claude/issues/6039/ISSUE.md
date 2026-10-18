# ESM-2026-10-18-EXT-04: FO4 / FO76 / Starfield WTHR cloud textures are never decoded

Labels: medium,esm-plugin,terrain-exterior,bug,game:fo4,game:fo76,game:starfield
Source: docs/audits/AUDIT_ESM_2026-10-18.md

- **Severity**: MEDIUM. All four cloud layers are `None` on every Creation-era weather.
- **Dimension**: Record Schema Dispatch
- **Record / Sub-record**: `WTHR` / `00TX`..`O0TX`
- **Location**: `weather.rs:444-694` (`parse_wthr`). Only Skyrim is routed to `parse_wthr_skyrim`, and the generic arm matches `DNAM`/`CNAM`/`ANAM`/`BNAM` only.
- **Status**: NEW
- **Description**: FO4, FO76 and Starfield use the Skyrim-style `xTX` cloud list (`wbWeatherCloudTextures` for non-FO3 games). The generic branch has no arm for it, and `parse_wthr_00tx_fourccs_are_inert` pins the inert behaviour for FO3/FNV only. `byroredux/src/scene/world_setup.rs:354-375` then resolves `cloud_textures[0..4]` as `None`.
- **Evidence**: FO4 census has `00TX` on 63 of 71 weathers, FO76 on 123 of 123, Starfield on 0 of 3 (it uses `WTHS`). `grep` shows no `xTX` handling outside `parse_wthr_skyrim`.
- **Impact**: FO4/FO76 exteriors get no authored cloud layers. These are later-roadmap games, but the decode gap is silent.
- **Related**: EXT-02, EXT-05.
- **Suggested Fix**: route FO4/FO76/Starfield `xTX` through the shared `skyrim_cloud_layer_index`, after EXT-05 fixes the index map, and fill `cloud_textures[0..4]`.

### LOW

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other games' arms, other callers)
- [ ] **TESTS**: A regression test pins this specific fix


_Filed from `docs/audits/AUDIT_ESM_2026-10-18.md` (`/audit-suite --area exterior`, HEAD 6f44874ac). Fix with `/fix-issue <number>`._
