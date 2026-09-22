# EXT-D5-2026-09-21-01: Skyrim/FO4 WATR noise-layer angles are read ~90° rotated from the record's own NAM0 frame

**Issue**: #4727
**Filed**: 2026-09-22 (audit-publish, AUDIT_EXTERIOR_2026-09-21.md)

**Severity**: HIGH. Wrong canonical value out of the WATAL translate, whole-game blast radius on Skyrim and FO4, no render-time fallback.
**Dimension**: Water translation (WATAL)
**Tier Violated**: no-fabrication (a mis-framed authored value is emitted as canonical scroll and current)
**Game Affected**: Skyrim (LE/SE), FO4. Starfield uses the same decode shape (DNAM 84/88/92 degrees) but was not verified.
**Location**:
- `byroredux/src/env_translate.rs:829-838` (`resolve_water_layer_motion`)
- `crates/plugin/src/esm/records/misc/water.rs:939-949` (Skyrim DNAM 100/104/108) and `:1144-1154` (FO4 DNAM 128/132/136)
- `env_translate.rs:533` (`WATER_CROSS_STREAM_SCROLL`) and `:900-918` (`confine_to_flow`) — the #4544 attenuation
- `docs/engine/watal.md:432-442` (records the offset as "authoring")

## Description
The `NAM0` current decodes as `[x, -y]` and matches all five direction-named Skyrim records within 10° of their editor-ID compass name. The per-layer `DNAM` angles are raw degrees read as a math angle in the same frame, with no documented rotation — measured against the same record's `NAM0`, they sit ~90° off. #4544 recorded this offset as authoring and attenuated it (`WATER_CROSS_STREAM_SCROLL = 0.25`), discarding ~70% of the authored speed profile while the residue still points mostly sideways.

## Evidence
Census over all 159 current-bearing layers (17 Skyrim records / 51 layers, 36 FO4 records / 108 layers): layer-minus-flow circular mean −84.1° (Skyrim, R=0.72, p≈6e-12) / −87.7° (FO4, R=0.65, p≈1e-19). A +90° rotation collapses the mean offset to +5.9° / +2.3°. Independent check against editor-ID compass names (`RiverWaterFlowNE/NW/SE`, `CreekWaterFlowSE/SW`) drops the layer-0 error from 45-102° to 6-45° under the +90° reading.

## Impact
Every Skyrim/FO4 water surface with authored layer motion scrolls on a ~90°-rotated field. River-classified records without a usable `NAM0` (Skyrim `RiverWaterFlow`, `CreekWaterFlow`; FO4 `ExtCreekSanctuaryWaterUVFlow`, `DLC04QuantumRiverWater[Int]`) take physics current from the same rotated angle, pushing floating bodies ~90° off course.

## Suggested Fix
Read the layer angle as `[-sin β, cos β]` (φ = β + 90°) at the translate/parse boundary. Drop or relax `WATER_CROSS_STREAM_SCROLL`, correct `watal.md`'s #4544 paragraph, and pin the census relation with a real-record test. Land together with the water.frag/water.vert scroll-sign fix — otherwise restored downstream layers render running upstream.

## Related
#4544 (closed; symptom patch), #2872, #3144

## Source
docs/audits/AUDIT_EXTERIOR_2026-09-21.md (EXT-D5-2026-09-21-01)
