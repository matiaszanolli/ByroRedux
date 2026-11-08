# ESM-5-01: Starfield EXPL / PROJ distances (and TXST DODT) decode at wire (metric) values, are not in `spatial_units::normalize`, and live systems read them as BU

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/6777
Labels: medium, esm-plugin, bug, game:starfield
Source: docs/audits/AUDIT_ESM_2026-11-08.md

_Filed from `docs/audits/AUDIT_ESM_2026-11-08.md` (audit HEAD 3500d2737)._

**Related**: Same regression class as closed #5001 / #5002 / #5151; introduced by closed #6679.

- **Severity**: MEDIUM
- **Dimension**: Cell/WRLD data + Starfield unit seam (checklist: "new Starfield distance fields join the lift")
- **Record / Sub-record**: Starfield `EXPL ENAM` (inner/outer radius), `PROJ PROD` (speed, range, collision radius, alt-trigger proximity), `TXST DODT` (decal width/height/depth)
- **Location**:
  - `crates/plugin/src/esm/records/spatial_units.rs:101-246` (no `explosions` / `projectiles` / `texture_sets` arm)
  - `crates/plugin/src/esm/records/misc/effects.rs` (`decode_starfield_prod`, `parse_expl` ENAM arm, added by `4b28775fd`)
  - `crates/plugin/src/esm/cell/base_objects.rs:120-127` (`DecalData`)
  - Consumers: `byroredux/src/systems/mines.rs:81-85,323` and `byroredux/src/systems/projectiles.rs:161-165`
- **Status**: NEW (regression class of #5002/#5001/#5151, introduced by #6679, closed)
- **Description**:
  - `normalize` lifts every other Starfield length from metres to BU (×70): positions, light radii, XCLL/LGTM/WTHR fog, water, LAND, NAVM, XRGD offsets.
  - #6679 made the Starfield `EXPL` radii and the `PROJ` speed, range, collision radius and alt-trigger proximity decode for the first time (they were 0 before), but nothing lifts them.
  - The mine system compares `position.distance(at)` (BU) to `proj.alt_trigger_proximity`, and uses `blast.inner_radius` / `blast.radius` as BU.
  - The projectile system uses `muzzle_speed`, `range` and `collision_radius` as BU.
  - `DecalData` is the same class, but it has no consumer yet ("no consumer exists", #3638).
- **Evidence** (Starfield.esm vs Fallout4.esm, raw bytes, scratchpad scan):
  - `TXST DODT` min width (n = 20 vs 207): median 4.0 vs 256.0.
  - `PROJ` collision radius (n = 265 vs 139): median 1.0 vs 5.0. Alt-trigger proximity > 0: median 5.0 vs 100.0.
  - Starfield `EXPL` (n = 273): median outer radius 8.0, inner 4.5. The #6679 test pins `fragGrenadeExplosion` at 3.5 / 8.0 and `FragMineProjectile` proximity at 2.0. These only make sense as metres (a 2-BU frag mine triggers at 3 cm, an 8-BU blast radius is 11 cm). `SpaceMineExplosion` / proximity 250 reads as a 250 m ship-scale mine.
  - The repo already lifts other Starfield lengths on comparable evidence (XRGD median 0.162 vs 15.33 BU, #5299).
  - Two vanilla rows look BU-like and may be leftovers: `ExplosionGrenadeShell` 100 / 256 and `pulseGrenadeExplosion` 150 / 500. That is why this is MEDIUM and not HIGH. The unit needs a per-row decision, which a lift guard test would force.
- **Impact**:
  - Starfield placed mines (1,268 per the #6679 message) fire at 2-5 BU (3-7 cm) and blast for 8 BU.
  - Starfield projectiles get a 70× too-short range and collision radius, and a muzzle speed that is probably 70× too slow (unverified: 800 BU/s is 11 m/s).
  - Starfield is only walkable in interiors, so the blast radius is bounded.
- **Suggested fix**:
  - Add `explosions` and `projectiles` arms to `normalize` (Starfield only): `inner_radius`, `radius`, `muzzle_speed`, `range`, `collision_radius`, `alt_trigger_proximity`. Lift only authored fields, per the #5301 rule.
  - Add a `DecalData` arm for `texture_sets` dimensions.
  - Pin it with a `starfield_expl_proj_distances_lift_once` test next to `starfield_index_lifts_lgtm_and_xcll_lighting_distances`.
  - Settle the two BU-looking `EXPL` rows by measurement before shipping.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related decoders / other games' arms
- [ ] **LOCK_ORDER**: n/a unless a RwLock scope changes
- [ ] **TESTS**: A regression test pins this specific fix (real-data pin where an install exists)
