# PHYSAL / Physics Audit — 2026-09-04 (scoped: Dimension 6 only, "water-deep" preset)

**Scope note**: this is a **scoped slice** of `/audit-physics`, run as one arm
of the `audit-suite --preset water-deep` run. It covers **only Dimension 6 —
"WATAL Physics Sink: Buoyancy, Damping, Current"** from the skill's 7-dimension
list. Dimensions 1-5 and 7 (shape translation, step determinism, ECS sync,
ragdoll articulation, character controller mechanics, queries/diagnostics)
were **not** audited in this pass — see `AUDIT_PHYSICS_2026-08-30.md` for the
most recent full 7-dimension pass, whose Dimensions 1/2/3/5/7 verdicts are not
superseded by anything here.

**Entry points audited**: `crates/physics/src/water.rs` (`PhysicsWaterConstants`,
`buoyancy_force`, `current_force`, `wind_force`, `submerged_fraction`,
`apply_buoyancy`/`apply_buoyancy_with_scratch`, `clear_stale_water_contacts`,
`waves_require_contact_rescan`), `crates/core/src/ecs/components/water.rs`
(`WaterPlane`, `WaterVolume`, `WaterCurrentVolume`, `WaterContact`,
`SubmersionState`, `WaterFlow`), `byroredux/src/commands/water.rs`
(`water.dump`, `water.contacts`), `byroredux/src/systems/character.rs` (the
`c7561d74` swim/drowning slice: `swimlevel_reached`, `swim_vertical_velocity`,
`advance_breath`, `apply_player_drowning_damage`), `crates/physics/src/sync.rs`
(phase order + the `n_new > 0` escape hatch), and the `#3492` Ragdoll-buoyancy
pass across `crates/physics/src/{components,ragdoll}.rs` +
`byroredux/src/ragdoll.rs`.

**Tests**: `cargo test -p byroredux-physics` — **156 passed, 0 failed,
0 ignored, 0 measured** (0.04s), 0 doc-tests. No compiler warnings observed
for this crate during the session.

**Method**: read every entry point directly (no sub-agent fan-out — this is a
single-dimension scoped run), cross-referenced against `git log` on the exact
file set since the last physics audit (`2026-08-30..HEAD`, i.e. through
`b15b0527`), and re-verified each SKILL.md checklist item against the current
code rather than trusting the prior report's text. Mandatory deduplication
protocol from `_audit-common.md` was run before writing anything below.

---

## Executive Summary

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 0 |
| LOW | 0 |
| **Total (new)** | **0** |

**Headline: the three open water findings this subsystem was carrying as of
the last physics audit are now all fixed, with regression tests, and nothing
new replaced them.**

| Issue | State at 2026-08-30 audit | State now (2026-09-04) |
|---|---|---|
| #3490 — current-volume Y test read body origin, not collider AABB centre | OPEN, verified true (PHYS-D6-2026-08-30-01 extended it) | **FIXED** — `0fd72cb6` (2026-09-03) |
| #3492 — ragdoll bones invisible to the buoyancy sink | OPEN, verified true | **FIXED** — `1e4d83a7` (2026-08-30) |
| #3494 — duplicated `#[test]` attribute / misattached doc comment | OPEN, LOW | **FIXED** — `802cae7b` (2026-09-02) |

Each fix was independently re-verified against HEAD rather than taken on
faith from the commit message — see the Verification Log below. All three
carry passing regression tests that specifically reproduce the original
defect's fixture (the offset-compound AABB-vs-origin trick used for both
#2887 and #3490; the Ragdoll double-registration test for #3492).

No new defect survived this pass's "attempt to disprove, only include what
survives" rule. One candidate (`wind_force`'s `WaterFlow` construction
bypassing the canonical `WaterFlow::new()` speed clamp) was investigated in
depth and dropped — see **Considered and Dropped** below; it reproduces
ground a prior audit (`AUDIT_PHYSICS_2026-08-20.md`, PHYS-D6-2026-08-20-01)
already covered and correctly dismissed.

### WATAL doctrine verdict (physics half) — **HOLDS**

```
$ grep -rn "GameKind|bsver|NifVersion|game_kind|is_skyrim|is_fo4|is_oblivion|game ==|BS_F76|SF_FORM_ID" \
    crates/physics/src/water.rs crates/core/src/ecs/components/water.rs byroredux/src/commands/water.rs
(no matches)
```

No per-game branch anywhere in the physics-side water code. `PhysicsWaterConstants`
remains a single engine-canonical resource (module doc, water.rs:9-12); the
per-game seam stays confined to the WATR/XCLW/XCWT parse+translate boundary,
which is `/audit-esm` Dim 5 / EXAL's territory, not this dimension's.

---

## Dimension 6 Checklist — Verified Against HEAD

| Checklist item | Verdict | Evidence |
|---|---|---|
| `submerged_fraction` clamps `[0,1]` and handles zero-height AABB | **Holds** | `height = (aabb_max_y - aabb_min_y).max(1e-6)` before the divide (`water.rs:281`); `submerged_fraction_clamps_and_survives_degenerate_aabb` |
| Archimedes lift proportional to submerged volume, opposed to gravity, renderer (Y-up) frame | **Holds** | `buoyancy_force` returns pure `+Y`; magnitude derived from `gravity_y.abs()` so sign convention can't leak; `buoyancy_force_scales_with_displacement_and_ratio` pins the neutral-equilibrium algebra |
| Wake discipline: buoyancy never pins the static-scene fast path; `n_new > 0` escape hatch present | **Holds** | `sync.rs:150` passes `n_new > 0` as `had_newcomers`; the quiescence gate at `water.rs:672-683` ORs in `!had_newcomers`; `buoyancy_survives_sub_tick_frames_above_sixty_fps` exercises it live |
| Current drag bounded, clamp + constant verified | **Holds** (water-current path) | `WaterFlow::new`/`for_kind` clamp `speed` into `SPEED_MIN..=SPEED_MAX` (0.5..=25.0 BU/s), documented as a hard ceiling for exactly this reason (#2872); `current_force` itself is a bounded first-order controller. See *Considered and Dropped* for the one non-canonical construction site (`wind_force`), which is inert at HEAD |
| `WaterContact` transition-frame contract (one dry frame on wet→dry) | **Holds** | Both in-loop exit arms (`water.rs:995-1006`, `:1008-1020`) and the all-surfaces-gone fast path (`clear_stale_water_contacts`, `:512-529`) write exactly one `WaterContact::default()`; `body_in_water_volume_floats_and_drifts_via_physics_sync` exercises the fast-path edge (#3127) |
| XCLW tri-state / render-half cross-check — report once, don't re-audit | **Pointer only** | `docs/engine/watal.md:28-30` (refreshed 2026-09-04) states the tri-state decode as landed; render-side water commits in the audited window (`c604375f`, `b15b0527`) touch only `water.frag`/`water.vert`/`render/water.rs`, none of this dimension's files |
| Character swimming + drowning (`c7561d74`) — audit, don't confirm absence | **Audited; holds** | `swimlevel_reached`, `swim_vertical_velocity`, `advance_breath`, `apply_player_drowning_damage` all read and traced (see Verification Log) |
| #3125 dt-correct swim damping still intact | **Holds** | `(-SWIM_DAMPING * dt).exp()` (per-second decay), not a per-tick constant multiplier; `swim_damping_is_frame_rate_independent` |
| #3128 zero-dt breath guard still intact | **Holds** | `advance_breath`'s `dt <= 0.0` branch preserves state instead of falling into the `!head_submerged` refill path (`character.rs:1080-1093`) |
| #3119 single death-reconciler for water hazard + drowning | **Holds** | Both `drowning_damage.whole` and authored `damage_per_second` route through the one `apply_player_drowning_damage`, whose only teardown call is `crate::combat::queue_dead_actor_reconciliation` — no second inline site found |

---

## Verification Log (why each "Holds" above isn't just a re-read of the doc comment)

- **#3490 fix** (`0fd72cb6`): before the fix, the current-volume containment
  test used `pos.y` (rigid-body origin) while the surface test 26 lines below
  it already used the collider AABB centre (`#2887`). Confirmed the diff
  hoists one shared `aabb_y` fetch above both branches and that the new test
  `current_volume_flow_is_measured_from_the_collider_aabb_centre_not_the_body_origin`
  (water.rs:1751) authors a current-volume band that contains only the AABB
  centre (not the origin) and asserts the body is actually pushed by it —
  i.e. the test can fail in the direction that would prove the bug is back,
  not just assert a tautology.
- **#3492 fix** (`1e4d83a7`): confirmed the structural argument the code
  comments make — `activate_ragdoll` removes both `RapierHandles` and
  `RigidBodyData` from every ragdoll bone (`byroredux/src/ragdoll.rs:433-455`),
  so a bone can never be double-counted by both the plain-dynamic-body scan
  and the new `Ragdoll` scan. Verified this is pinned by
  `activation_tears_down_keyframed_bone_bodies` in the same file (asserts both
  components gone post-activation, and that a re-run of `physics_sync_system`
  does not re-register them).
- **#3494 fix** (`802cae7b`): `cargo test -p byroredux-physics` produced zero
  warnings and 156/156 passing in this session — consistent with the fix
  being real, not just a commit message.
- **Doctrine grep**: re-run fresh in this session (not copied from the prior
  report), same zero-match result.

---

## Considered and Dropped

### `wind_force`'s `WaterFlow` construction bypasses the canonical `WaterFlow::new()` speed clamp

- **Files**: `crates/physics/src/water.rs:229-254` (`wind_force`), read
  against `crates/core/src/ecs/components/water.rs:466-501` (`WaterFlow::new`,
  the documented sole canonical constructor) and
  `crates/core/src/ecs/components/groundcover.rs:245-269` (`WindField::from_weather_byte`).
- **What's true**: `wind_force` builds `WaterFlow { direction: [...], speed:
  gust }` as a raw struct literal, not via `WaterFlow::new`, so the module's
  own documented invariant — "every producer must come through here... an
  unclamped value is an unbounded velocity target (#2872)" — does not apply
  to the wind branch. `gust` can reach up to ~396 BU/s at max storm intensity
  (`speed` ≤ `MAX_WIND_SPEED` = 220, `gust_amplitude` ≤ `speed * 0.8` = 176),
  roughly 16× `WaterFlow::SPEED_MAX` (25.0).
- **Why it's not filed as a finding**:
  1. `AUDIT_PHYSICS_2026-08-20.md` already investigated this exact code region
     under PHYS-D6-2026-08-20-01 and concluded: *"the functions are first-order
     and bounded... the unboundedness is in the application, not the math."*
     That conclusion still holds — `current_force` is a proportional
     controller that converges velocity toward `flow.speed` and never
     overshoots it, independent of the magnitude of `speed`.
  2. Unlike a WATR-authored `WaterFlow.speed` (parsed from arbitrary file
     bytes — exactly what #2872's clamp defends against), `gust`'s only
     production source is `WindField::from_weather_byte`, whose input is a
     `u8` run through a fixed formula — bounded by construction, not by a
     clamp that untrusted content could route around. Grepped for every other
     production `WindField { .. }` literal; found none outside
     `from_weather_byte` / `WindField::CALM` / tests.
  3. At the observed ceiling, the worst-case wind acceleration on a floating
     body (`speed_error * wind_drag ≈ 396 * 0.35 ≈ 139 BU/s²`) is well under
     gravity's `686.7 BU/s²` — nowhere near "launched out of the water."
- **Verdict**: real but currently-inert inconsistency between a documented
  module invariant and one call site. Does not survive the "attempt to
  disprove" bar for a numbered finding, and re-treads ground a prior audit
  already covered by a different route. Worth a one-line comment at the
  `wind_force` call site someday explaining why the bypass is safe — noted
  here for the next auditor rather than filed.

---

## Known-Open Register

Restated per Phase 3's merge instructions.

### The three don't-re-litigate items (untouched by this pass, as expected — none are Dim 6's territory)

| Item | State after this pass |
|---|---|
| **`tes_grounding_zero_mass_dynamic_fix`** — Skyrim mass=0 Dynamic bodies reclassified Static (#1832) | Not touched. Outside Dim 6's file set; door-threshold spawn gap (Dim 5's territory) unchanged. |
| **`interior_spawn_point_fix`** — interiors spawn at the first door's own placement | Not touched. No assumption of auto spawn-point logic was introduced anywhere in this pass. |
| **`fnv_furniture_sit_needs_transition`** — sit loops have no pelvis/root channel, gated behind `BYRO_SANDBOX_SIT` | Not touched. Out of this subsystem's path. |

### Water-adjacent items from prior audits — resolved by this pass's window

| # | Prior state | State now |
|---|---|---|
| #3490 | OPEN (MEDIUM/LOW depending on report) | **CLOSED, fix verified** |
| #3492 | OPEN (implicit MEDIUM — dropped ragdoll buoyancy) | **CLOSED, fix verified** |
| #3494 | OPEN (LOW) | **CLOSED, fix verified** |

### Still open, not this dimension's to fix, restated for continuity

| # | Sev | Owner | Note |
|---|---|---|---|
| #3477 | MEDIUM | `/audit-performance` Dim 1 | `collect_newcomers` still rescans every collider row per tick — verified still OPEN in `gh issue list`, not re-investigated here (Dim 7/perf territory, not Dim 6) |
| water-walking, freezing, Skyrim DNAM tail decode, cross-game visual smoke matrix | — | Documented open features (`docs/engine/watal.md:438`, refreshed 2026-09-04) | Not bugs; not filed |

---

## Cross-Audit Deduplication

| Topic | Owner |
|---|---|
| XCLW tri-state / WATR DNAM decode | `/audit-esm` Dim 5 |
| Water rendering, wave shader, reflection/foam/shoreline | `/audit-renderer` Dim 15 |
| `collect_newcomers` per-tick rescan (#3477) | `/audit-performance` Dim 1 |
| Ragdoll articulation / constraint CInfo decode (Dim 4, not re-run here) | `/audit-physics` Dim 4 — see `AUDIT_PHYSICS_2026-08-30.md` |
| Character controller mechanics (grounding, KCC) (Dim 5, not re-run here) | `/audit-physics` Dim 5 — see `AUDIT_PHYSICS_2026-08-30.md` |
| `unsafe` blocks | `/audit-safety` (this subsystem contributes none in the audited file set) |

---

## Publish

No findings to publish from this pass — nothing to run `/audit-publish`
against for Dimension 6. If the parent `water-deep` suite run wants a
consolidated ledger across its other arms, merge this report's empty finding
set with theirs rather than invoking `/audit-publish` on this file alone.
