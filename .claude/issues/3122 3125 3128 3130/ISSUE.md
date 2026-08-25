# Issues 3122, 3125, 3128, 3130 — Physics (PHYSAL) audit findings, 2026-08-20

All four originate from `docs/audits/AUDIT_PHYSICS_2026-08-20.md`.

## #3122 (MEDIUM, bug+performance) — apply_buoyancy fast path unreachable in shipping binary
- `crates/physics/src/water.rs:484-511` — `waves_active` gate is effectively always true
  (TotalTime always present at boot; WaterMaterial::wave_amplitude default 0.05 >> 1e-4 threshold),
  so the "quiesced scene" fast path never engages in the real binary.
- Regression test `buoyant_body_sleeps_so_static_fast_path_re_engages` (water.rs:1296) doesn't
  insert TotalTime, so it tests an unreachable config.
- Fix: choose (a) drop dead waves_active gate + cheaper reachable check (surfaces non-empty AND
  some body has WaterContact/awake), or (b) event-driven wave following (only bodies with
  WaterContact). Insert TotalTime into the test. Restate docstring precondition.

## #3125 (MEDIUM, bug) — swim_vertical_velocity frame-rate dependent damping
- `byroredux/src/systems/character.rs:964-984` — `prev_velocity * 0.72 + spring * dt` mixes
  dt-free per-frame decay (0.72) with dt-scaled spring term.
- Fix: replace 0.72 with dt-correct exponential decay `exp(-SWIM_DAMPING * dt)` calibrated so
  `SWIM_DAMPING ≈ 19.7` reproduces 0.72 at 60fps. Add multi-dt regression test.
- Sibling check: jump branch at :975-978 also has `prev_velocity * 0.15` raw term — check too.

## #3128 (LOW, bug) — advance_breath refills breath reserve on zero-dt tick
- `byroredux/src/systems/character.rs:994-1010` — `!head_submerged || dt <= 0.0` collapses two
  cases; dt<=0.0 should be a no-op, not full refill.
- Fix: split branches — `!head_submerged` → (MAX_BREATH, zero); `dt <= 0.0` → preserve previous
  breath/remainder.

## #3130 (LOW, documentation/doc-rot) — pull_dynamic stale lock-ordering comment
- `crates/physics/src/sync.rs:1075-1080` — comment describes drops that now happen ~85 lines
  earlier (sync.rs:1002-1003, post #2404 restructure). Move comment to the actual drop site;
  reword to state invariant directly (Transform write never taken while RapierHandles/
  RigidBodyData read guard live) — preserve #2135 ABBA-risk mention.

## Domain
All four → `byroredux-physics` crate (Havok→Rapier physics layer) + #3125/#3128 also touch
`byroredux` binary (`byroredux/src/systems/character.rs`).

## Resolution

- **#3125, #3128, #3130** — fixed in this pass (see commit).
- **#3122 — already fixed, closed without a new commit.** It's a duplicate of #3135
  (same root cause, filed from the performance-audit angle, already CLOSED). #3135's fix
  landed in `d628acfc` ("fix(watal): restore buoyancy quiescence", 2026-08-21) — replaced
  the dead `waves_active` gate with `waves_require_contact_rescan`, which only requires a
  rescan when a live `WaterContact` exists near the surface, so a water cell with nothing
  floating in it still takes the quiesced-scene fast path. Verified: unit test
  `default_authored_waves_only_rescan_nearby_live_contacts` (`water.rs:1008`) pins exactly
  this — a settled cell running `WaterMaterial::default()` amplitude does NOT force a
  rescan; a live contact inside the crest band does. Full `byroredux-physics` suite
  (148 tests) passes at HEAD.
  - The `buoyant_body_sleeps_so_static_fast_path_re_engages` test's lack of `TotalTime`
    is not a masking bug in practice: it pins a *different* invariant
    (`PhysicsWorld::step`'s own no-awake-bodies fast path / wake discipline), which never
    depended on `waves_active`/`waves_require_rescan` — the wake decision is driven solely
    by the ≥0.1 BU depth-delta check at `water.rs:679-684`, unaffected by whether the scan
    itself runs every frame. No SIBLING gap found in the other two physics fast-path tests
    (`world.rs` — `wake_re_engages_stepping`, `per_frame_forces_can_be_applied_without_arming_the_fast_path`);
    neither touches `TotalTime`/waves.
