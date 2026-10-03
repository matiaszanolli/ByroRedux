# #5169: EXT-D1-2026-10-02-01: FO76 shares Starfield's WATR decoder and WATAL translate, but #5151/#4837 settled units on Starfield data only — FO76's per-metre absorption triplet reaches WATAL as per-BU (~70× too opaque), and its lane 3 saturates the oceanness term

**Labels**: high,terrain-exterior,water,esm-plugin,bug,game:fo76
**Source**: docs/audits/AUDIT_EXTERIOR_2026-10-02.md

- **Severity**: HIGH (divergent canonical value out of the WATAL translate: the same authored numbers in FO76 and Starfield produce values 70× apart). Live impact is latent, because FO76 is "Parse only" in the compat matrix.
- **Dimension**: EXAL boundary discipline (unit lift completeness). Overlaps Dim 5 (WATAL).
- **Location**:
  - `crates/plugin/src/esm/records/spatial_units.rs:67` (Starfield-only gate) and `:151-181` (waters arm);
  - `crates/plugin/src/esm/records/misc/water.rs:1263-1265` (`decode_dnam_fo76` → `decode_dnam_starfield`) and `:1283-1299` (absorption at 4/8/12, concentration at 16/20/24/28);
  - `byroredux/src/env_translate.rs:889-918`;
  - consumers `crates/renderer/shaders/water.frag:576-584` (`exp(-hitDist * coeff)`, hitDist in BU), `:571`, `:1246` (oceanness), and `byroredux/src/systems/water.rs:593-613` (`underwater_color_at_depth`).
- **Status**: NEW. It is the unverified half of #5151's SIBLING check ("FO76 … stays in BU"). That check was settled on FO76's distance lanes (noise falloff 4096, underwater fog −9000/850) and never on the inverse-length lane.
- **Tier Violated**: no-fabrication (a unit was assumed for a lane without a census).
- **Game Affected**: Fallout 76.
- **Description**:
  - #5151 divides Starfield's absorption triplet by 70 because the values are per-metre extinction. Its own evidence: red > green > blue, with magnitudes that match liquid water.
  - FO76 runs the byte-identical decoder and the same game-agnostic translate, but is excluded from the lift. Its triplet therefore reaches `exp(-hitDist_BU * coeff)` unconverted.
  - #4837 then passes lane 3 through `clamp(0,1)` on the strength of a Starfield-only census.
- **Evidence** (census, SeventySix.esm + NW.esm, 47 WATRs, every one 148-B DNAM):
  - All 47 author a non-zero absorption triplet; the median red is 0.207.
  - 5 records author **0.3 / 0.075 / 0.01**: `ExtTroughWater`, `IntTroughWater`, `ExtTestWater`, `DEBUG_ExtRiverCharlesUpper`, `ExtCranBogWaterFlow`.
    - That is exactly Starfield's `WaterSulfuric` / `SFBGS001_WaterVaruunWok_*` triplet, which #5151 now divides by 70.
    - It is also the pure-water absorption spectrum per metre (Pope & Fry: ~0.34 at 650 nm, 0.06 at 550 nm, 0.009 at 450 nm).
  - `Burn_ExtAbraxoWaterBasin` authors Starfield's 0.07627 blue value.
  - Read per BU, FO76's "clear" `ExtClearWaterPuddle` (0.24/0.18/0.2) reaches 1/e at ~4–5 BU (≈7 cm), and a trough's red channel at 3.3 BU (≈5 cm). Read per metre, both are plausible (1/e at 3–5 m).
  - Oceanness: FO76 lane 28 spans 0.16–75.72 (median 4.0; 39/47 > 1.0). After #4837, 39/47 FO76 waters saturate to 1.0, which gives +0.25 density and +50 % forward scatter.
  - FO76's pigment lanes (16/20/24) are 9e-5–0.52, not Starfield's 0–20, so `/STARFIELD_WATER_CONCENTRATION_REFERENCE` (`env_translate.rs:911-915`) reduces them to ~0.
- **Impact**:
  - Any FO76 water surface or underwater view would go to the deep tint within centimetres, the same symptom #5151 fixed for Starfield.
  - The oceanness/pigment terms are driven by a scale nobody measured for FO76.
  - No gate renders FO76 water.
- **Suggested Fix**:
  - Settle FO76's inverse-length unit from the shared-triplet census above. If it is confirmed, extend the absorption ÷70 to FO76: either in `spatial_units` with a FO76 arm limited to the triplet, or by having `decode_dnam_fo76` stop sharing the inverse-length convention silently.
  - Census FO76 lanes 16–28 before applying the Starfield concentration semantics. Until then, zero them for FO76 at the decoder (the documented "absent" sentinel).
  - Pin both with a FO76 DNAM fixture next to `fo76_index_watr_keeps_authored_units`. Today that test asserts the unconverted triplet `[0.16558, 0.096239, 0.076271]` (`spatial_units_tests.rs:269`), which locks in the questionable behaviour.

**Source report**: `docs/audits/AUDIT_EXTERIOR_2026-10-02.md` (HEAD `c9f95283a`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other games sharing the decoder / other consumers of the same field / other copies of the stale text)
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the EXAL/WATAL `translate_*` (or `spatial_units::normalize`) boundary — never pushed into shaders/renderer, never re-derived at render time
- [ ] **TESTS**: A regression test pins this specific fix
