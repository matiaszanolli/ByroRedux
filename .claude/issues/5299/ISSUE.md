# #5299: ESM-2026-10-05-D5-01: Starfield `XRGD` ragdoll-pose positions are metric, but `spatial_units::normalize` never lifts them — a new Starfield distance field outside the ×70 lift

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/5299
- **Labels**: low,esm-plugin,bug,game:starfield
- **Source**: `docs/audits/AUDIT_ESM_2026-10-05.md` (ESM-2026-10-05-D5-01)

_From `docs/audits/AUDIT_ESM_2026-10-05.md` (2026-10-05 comprehensive audit suite, HEAD a2c24b16e)._

- **Severity**: LOW. The field is decoded only; posing is still open in #5015.
- **Dimension**: CELL / WRLD Walkers & Placement Data
- **Record / Sub-record**: `REFR`/`ACHR` / `XRGD`
- **Location**: `crates/plugin/src/esm/records/spatial_units.rs:42-56` (`fn cell`, no `ragdoll_pose` lift); the field was added at `crates/plugin/src/esm/cell/mod.rs` `PlacedRef::ragdoll_pose` and decoded in `cell/helpers.rs::decode_ragdoll_pose`
- **Status**: NEW. Related to open #5015, which is decode-only.
- **Description**: `cell()` lifts every Starfield placement length: position, teleport, primitive bounds, water velocity and radius override. The new `ragdoll_pose[*].position` (xEdit `wbRagdoll` → `wbVec3PosRot`, `Common:9407`) is not in that list. The skill's Dim 5 rule says new Starfield distance fields join the lift and dimensionless ones (Euler angles) stay out; the rotations here are correctly left alone.
- **Evidence**: XRGD census (`scripts/xrgd.py`):

  | master | placements with XRGD | median entry offset |v| | p90 |
  |---|---|---|---|
  | `Starfield.esm` | 31,886 (30,016 REFR + 1,870 ACHR) | 0.162 | 0.576 |
  | `Fallout4.esm` | 8,919 | 15.33 BU | 33.09 BU |

  Every Starfield XRGD is a multiple of 28 bytes. The FO4 median ÷ 70 = 0.219, so Starfield's offsets are metres, the same factor as every other lifted lane.
- **Impact**: None until a pose consumer lands. At that point every Starfield corpse and settled-clutter pose would be compressed about 70× toward its root.
- **Related**: #5015 (open), #5151 / #5134 / #5002 (the same lift family, closed).
- **Suggested Fix**: In `cell()`, call `vector(&mut bone.position)` for each `refr.ragdoll_pose` entry, beside `refr.position`. Add a `spatial_units_tests` case for a Starfield XRGD and a legacy-game control.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
