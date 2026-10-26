# EXT-D5-2026-10-26-02: Shore-refraction tuning literals are retyped in `water.vert` and the CPU crest sampler instead of owned in core

Labels: low,water,shaders,physics,bug
Source: docs/audits/AUDIT_EXTERIOR_2026-10-26.md

- **Severity**: LOW
- **Dimension**: Water translation (WATAL)
- **Tier Violated**: single-boundary (the skill's "retyped literal in the shader and the CPU crest sampler is a finding" rule)
- **Game Affected**: all (calm water near a shore)
- **Location**: `crates/renderer/shaders/water.vert:204-216,266-277` vs `crates/physics/src/water.rs:489-496,522-528,576-577`; `crates/core/src/ecs/components/water.rs:891,899`
- **Status**: NEW
- **Description**: The bed-shape constants are owned in core (`WATER_SHORE_REFRACT_DEPTH` / `_MAX`, `WATER_SWELL_*`, `WATER_RIVER_*`). Four shape parameters sit beside them as bare literals, mirrored by hand:
  - The `0.6` share by which the ripple pair (`dirA`/`dirB`) turns toward the shore: `refraction * 0.6` in `water.vert` against `refraction * 0.6` in `physics/water.rs:576-577`. It is the only tuning knob without a named const.
  - The `0.5` upper edge of the swell's "travelling toward shore" ramp: `smoothstep(0.0, 0.5, dot)` against `cosine / 0.5` in `refract_swell`.
  - The `0.1` minimum shoreward-vector length: `uphillLen > 0.1` in `water.vert:204,210` against `shore_len > 0.1` in `physics/water.rs:522`, plus `bank_len > 0.1` in `WaterFlow::over_bed`.
  - The `0.25` inner-edge fraction (`0.25 * WATER_SHORE_REFRACT_DEPTH`, `0.25 * WATER_RIVER_BANK_DEPTH`). This one is structural to the smoothstep form.
- **Evidence**: The text pin `swell_shore_and_river_shaders_keep_their_contract` checks only the swell phase/shoal needles, not these. A retune on one side would silently desync the visible and buoyancy crest direction.
- **Impact**: No current mismatch; the values agree today. It is a drift trap on exactly the CPU/GPU parity the #5582 rule exists to prevent.
- **Related**: #5582 (`7cf2bd086`).
- **Suggested Fix**: Add `WATER_SHORE_RIPPLE_TURN_SHARE`, `WATER_SWELL_SHORE_TOWARD_RAMP` and `WATER_SHORE_MIN_UPHILL_LEN` to core `water.rs`, re-export them through `shader_constants_data.rs`, and use the names in both places.

_Filed from `docs/audits/AUDIT_EXTERIOR_2026-10-26.md` (HEAD 3e6f6b66a). Static trace; not confirmed on screen unless stated._

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader copies, other sun/lighting sites, other rings)
- [ ] **TESTS**: A regression test pins this specific fix
