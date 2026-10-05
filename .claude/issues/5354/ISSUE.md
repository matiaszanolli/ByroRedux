# #5354: PHYS-D5-2026-10-05-01: #5245's flowing-water weather damping reached the renderer and `submersion_system` but not dynamic buoyancy or `player_water_state`, so physics crests on rivers diverge from the visible surface

**Labels**: medium,physics,water,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5354

**Source**: `docs/audits/AUDIT_PHYSICS_2026-10-05.md` — `PHYS-D5-2026-10-05-01` (HEAD `a2c24b16e`)

- **Severity**: MEDIUM
- **Dimension**: Water / Buoyancy
- **Location**:
  - `crates/physics/src/water.rs:650` (scroll sampled), `:888-897` (buoyancy crest sample, raw scroll)
  - `byroredux/src/systems/character.rs:1205-1235` (`player_water_state`, raw scroll)
  - Compare `byroredux/src/systems/water.rs:212-230` and `byroredux/src/render/water.rs:236-247` (damped)
  - Constant: `crates/core/src/ecs/components/water.rs:150-163`
- **Status**: NEW (regression of the #3207 phase-coherence contract, introduced by `37cc637e9` #5245/#4929)
- **Trigger Conditions**: a `WaterPlane` whose `kind.is_flowing()` (River, Rapids, Waterfall), with a non-zero
  `WindField` gust, and a dynamic body or the player near its surface.
- **Description**: `FLOWING_WATER_WEATHER_TRANSPORT` (0.35) scales the weather scroll on flowing kinds. Its doc
  says it is "Applied identically by the renderer upload and the CPU crest sampler so the #3207 phase-coherence
  contract holds".
  - `grep -rn "FLOWING_WATER_WEATHER_TRANSPORT\|is_flowing()"` finds it in only two places: `render/water.rs` and
    `systems/water.rs` (camera submersion).
  - `apply_buoyancy_with_scratch` and `player_water_state` call `authored_wave_height_with_weather` with the raw
    `weather_wave_adjustment` scroll.
  - That function adds the weather scroll to `scroll_a`/`scroll_b` *before* deriving `dir_a`/`dir_b` and
    `rate_a`/`rate_b` (`water.rs:362-379`). The two paths therefore produce different crest directions and speeds,
    not just different phases.
  - On the #5245 White River fixture, the undamped weather is about 0.33 UV/s against a downstream flow term of
    about 0.13. The physics crests travel with the wind, while the rendered crests, and the waterline the camera's
    underwater state is tested against, travel downstream.
- **Evidence**:
  ```rust
  // byroredux/src/systems/water.rs:216 — damped (camera submersion)
  let weather_scroll = if plane.kind.is_flowing() { [ws[0] * WaterKind::FLOWING_WATER_WEATHER_TRANSPORT, …] } else { ws };
  // crates/physics/src/water.rs:890 — buoyancy: `weather_scroll` passed straight through
  // byroredux/src/systems/character.rs:1228 — player: `weather_scroll` passed straight through
  ```
- **Impact**:
  - Floating debris and ragdolls on rivers bob against a crest pattern the player cannot see.
  - The player's swimlevel and `WaterContact` depth are computed against a different crest than the camera's
    `SubmersionState`. Near the waterline, the swim state and the underwater tint and audio can disagree by up to
    one wave amplitude × `wind_wave_scale`.
  - The skill's rule, "Every water input added to one [sampler] must exist in the other", is violated for a whole
    class of water.
- **Related**: #5245 / #4929 (closed); #3207; PHYS-D5-2026-09-29-01 / #5129 (the previous sampler-parity
  instance). `/audit-exterior` owns the policy constant; the sampler parity is reported here, as the skill
  requires.
- **Suggested Fix**: move the damping into one helper, for example a `WaterKind`-aware
  `weather_scroll_for(kind, scroll)` next to `weather_wave_adjustment`. Call it from all four samplers: renderer,
  submersion, buoyancy (via `WaterSurface`, which would need the plane's `kind`) and the player. Add a parity test
  that samples all three CPU paths on a River plane with wind.

## Publisher note

Regression of the #3207 phase-coherence contract (not of a single closed issue), introduced by `37cc637e9` (#5245/#4929, both closed). `/audit-exterior` (`AUDIT_EXTERIOR_2026-10-05.md`, cross-audit routing) agrees: the 0.35 policy constant is a documented engine choice; fix the parity as one helper next to `weather_wave_adjustment` called by `render/water.rs`, `systems/water.rs`, `systems/character.rs` and `crates/physics/src/water.rs`. Also noted, not filed: the buoyancy wave sample uses the body origin `pos` while `surface_y_at` uses `reference_point` (`crates/physics/src/water.rs` ~884 vs ~892) — fix alongside if that sampler is touched.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, sibling call sites)
- [ ] **TESTS**: A regression test pins this specific fix
