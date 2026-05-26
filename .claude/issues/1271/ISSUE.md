# F-FO3-D3-01: WATR.NNAM (FO3/FNV noise-texture path) silently dropped

## Severity: High

**Location**: `crates/plugin/src/esm/records/misc/water.rs:273-309` (`parse_watr`)

## Problem

FO3 and FNV `WATR` records ship the noise/diffuse texture path in the **`NNAM`** sub-record (a zstring like `Data\Textures\Water\WastelandWaterPotomac.dds`). Skyrim+ uses `TNAM` for the same purpose. `parse_watr` only reads `TNAM`, so `WatrRecord.texture_path` is **empty on 100% of FO3 WATR records (0 of 53) and 100% of FNV WATRs (0 of 78)**.

## Evidence

Sub-record histogram over `Fallout3.esm`:

```
FO3: WATR records = 53
  EDID  53  (100%)
  FNAM  53  (100%)
  ANAM  53  (100%)
  DATA  53  (100%)
  DNAM  42  (79%)
  FULL  38  (71%)
  GNAM  53  (100%)
  MNAM  53  (100%)
  NNAM  53  (100%)   <-- 100% of records ship NNAM
  SNAM  7   (13%)
  XNAM  51  (96%)
```

Sample decoded NNAM payloads:

| WATR EDID | NNAM |
|---|---|
| `PotomacNRShallow` | `Data\Textures\Water\WastelandWaterPotomac.dds` |
| `DCMallWater` | `Data\Textures\Water\WastelandWaterPotomac.dds` |
| `WaterTypeMegatonWater` | `Data\Textures\Water\WaterFlowRippleNoise01.dds` |
| `WaterTypeOasisClean` | `Data\Textures\Water\WaterFlowRippleNoise01.dds` |
| `ReflectingPoolWaterType` | `""` (intentionally blank — reflecting pool) |

Zero FO3/FNV records ship `TNAM`.

## Impact

Every FO3 and FNV water plane renders without its authored noise/displacement map. The cell-loader consumer at `byroredux/src/cell_loader/water.rs:286` reads `record.texture_path == ""` and falls through to either the per-water default (`textures/water/water_default.dds`) or — more likely — the magenta missing-texture placeholder for `WaterPlane`. Affects every FO3/FNV water body: Potomac, Megaton water tower, Oasis, Rivet City, Reflecting Pool, Lake Mead, Hoover Dam, Quarry Junction ponds, the cazador swamps, every Vegas pool.

## Fix

In `parse_watr` at `crates/plugin/src/esm/records/misc/water.rs:282`, add an arm adjacent to the existing `TNAM` arm:

```rust
b"NNAM" => out.texture_path = read_zstring(&sub.data),
```

NNAM and TNAM are game-mutually-exclusive on Bethesda titles (FO3/FNV ship NNAM exclusively; Skyrim+ ships TNAM exclusively), so a last-arm-wins match is safe without a `GameKind` gate. Add a regression-test fixture using `PotomacNRShallow` as a clean candidate (tests at `water.rs:321-449`).

## Completeness Checks

- [ ] **TESTS**: Regression test using a real FO3 WATR fixture (`PotomacNRShallow` payload) verifies non-empty `texture_path` post-fix
- [ ] **SIBLING**: Verify Skyrim+ `TNAM` path still resolves correctly (last-arm-wins should be a no-op there since FO3/FNV don't author TNAM)
- [ ] **VALIDATION**: Re-run `parse_real_esm` against both `Fallout3.esm` and `FalloutNV.esm`; assert non-empty `texture_path` on the WATR maps where NNAM is non-empty
- [ ] **CONSUMER**: Confirm `byroredux/src/cell_loader/water.rs:286` actually consumes the populated path (texture-load path may have its own gaps — separate dim)

Related: #1069 (`F-WAT-09: WATR reflection_color parsed but never propagated`) — separate WaterParams pipeline gap, closed. This is a different field (texture path vs reflection colour).

Audit: `docs/audits/AUDIT_FO3_2026-05-25_DIM3.md` (F-FO3-D3-01)
