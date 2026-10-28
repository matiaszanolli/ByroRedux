# EXT-D7-2026-10-27-01: The FO4 seam exemption matches the vanilla crack's counts, not its cells

Labels: low, terrain-exterior, test-gap, bug, game:fo4
Source: docs/audits/AUDIT_EXTERIOR_2026-10-27.md

**Source**: `docs/audits/AUDIT_EXTERIOR_2026-10-27.md` finding `EXT-D7-2026-10-27-01` (Dimension 7, gates) · Severity LOW (test-gap) · Status NEW (`6209c213e`) · Game: FO4.

**Location**: `docs/smoke-tests/m-exteriors.sh:835-845`; `crates/core/src/ecs/resources/mod.rs:1440-1452` (`TerrainSeamStats::machine_line`).

## Description
The `fo4` profile passes the terrain-seam check on `pairs_dirty == 1 && height_mismatch_vertices == 3`. The `terrain-seams:` machine line carries no pair coordinates, so the gate cannot check that the dirty pair is LAND (3,0)/(4,0) (the known stock Fallout4.esm crack, #3306). The script comment claims "a new defect cannot hide behind the known one". That holds only when the known pair is resident in the run. The boundary scenario moves the player, so a run without (3,0)/(4,0) resident that contains one new 3-vertex crack passes as the known crack. The same happens if a mod masks or fixes the vanilla pair.

## Evidence
```bash
elif [[ "$label" == fo4 ]] && (( seam_dirty == 1 && seam_height_mismatch == 3 )); then
    # #3306 — stock Fallout4.esm ships exactly one localized crack: LAND (3,0)/(4,0) ...
```
```rust
"terrain-seams: sampled={} pairs_checked={} pairs_dirty={} \
 height_mismatch_vertices={} normal_mismatch_pairs={} verdict={}"
```
No coordinates appear in the machine line.

## Impact
A new 3-vertex terrain crack in an FO4 run where the known pair is not resident is silently exempted; the gate's stated guarantee does not hold.

## Related
#3306 (closed; the known crack), `vanilla_fo4_commonwealth_seam_tests`.

## Suggested Fix
Add the dirty pair list (or the first pair's cells) to the machine line and match `(3,0)/(4,0)` in the exemption.

## Completeness Checks
- [ ] **SIBLING**: Other consumers of `TerrainSeamStats::machine_line` (README/debug-cli docs, `terrain.seams` console command) updated for the new field
- [ ] **TESTS**: A unit test pins the new machine-line format; the exemption is exercised with a non-matching pair

