### PHYS-D1-2026-09-11-02: Character-controller/ground-probe capsule shapes use a floor-only `.max(1e-3)`, not `clamp_shape_extent`

- **Severity**: LOW
- **Dimension**: Shape Translation
- **Location**: `crates/physics/src/world.rs:986`, `:1184-1185`
- **Status**: NEW
- **Source**: `docs/audits/AUDIT_PHYSICS_2026-09-11.md`

**Description**: these two sites build an ephemeral `SharedShape::capsule_y` for a character-controller move / ground-probe cast — not a persistent `CollisionShape`-derived collider, so they never go through `collision_shape_to_parts` (which is fully clean per the audit's Dimension 1 checklist). They are the only other `capsule_y(...).max(1e-3)` pattern in the crate with no ceiling clamp analogous to `clamp_shape_extent`. The values trace back to `CharacterController`'s engine-authored HUMAN-preset constants — not untrusted NIF/ESM content — so practical exploitability today is low, but there is no ceiling the way `clamp_shape_extent` provides everywhere else, and project memory records a planned CHARAL-driven per-race height scale that would make these values data-derived.

**Evidence**: `SharedShape::capsule_y(capsule_half_height.max(1e-3), capsule_radius.max(1e-3))` at both `crates/physics/src/world.rs:986` (`cast_capsule_down_surface_and_normal`) and `crates/physics/src/world.rs:1184-1185` (character-controller move), vs. the guarded `clamp_shape_extent` used throughout `convert.rs`. `INFINITY.max(1e-3) == INFINITY` under IEEE `maxNum` semantics (NaN is already handled correctly), so an unbounded value would pass through unclamped.

**Impact**: none currently observable; a future data-driven height value without validation could hand the broad-phase an unbounded capsule for character-controller sweeps specifically.

**Related**: none — distinct from the `CollisionShape` producer family this audit's other findings concern.

**Suggested Fix**: if/when character capsule dimensions become data-derived, route them through `clamp_shape_extent` or an equivalent; not urgent while the values are engine constants.

## Completeness Checks
- [ ] **SIBLING**: When CHARAL per-race height sizing lands, check both call sites in the same change
- [ ] **TESTS**: Add a regression test asserting a pathological (`INFINITY`/very large) capsule dimension is clamped before reaching `SharedShape::capsule_y`
