# PHYS-D6-2026-08-20-04: apply_buoyancy's quiesced-scene fast path is unreachable in the shipping binary — its regression test only passes because the test world has no TotalTime

**Issue**: #3122 — https://github.com/matiaszanolli/ByroRedux/issues/3122
**Finding**: `PHYS-D6-2026-08-20-04`
**Labels**: bug, medium, performance
**Filed**: 2026-08-20 (comprehensive `/audit-suite` sweep, 25 reports)

---

**Audit**: `docs/audits/AUDIT_PHYSICS_2026-08-20.md` — Dimension 6 (Water / Buoyancy)
**Severity**: MEDIUM · **Status**: NEW — the gate was widened with `&& !waves_active` by `a70f80d9` / `6b960349` (`follow authored waves in buoyancy contacts`) this cycle

## Location
- `crates/physics/src/water.rs:484-487` — `waves_active`
- `crates/physics/src/water.rs:497-511` — the fast path
- `crates/physics/src/water.rs:1296-1382` — the test that pins it

## Trigger conditions
Every frame of every cell that has at least one `WaterPlane` — i.e. the entire delta's target workload.

## Description
The fast path is gated on `awake_counts().0 == 0 && !pending_wake() && !had_newcomers && !waves_active`, and `waves_active` is:

```rust
let waves_active = time_secs.is_some()
    && surfaces.iter().any(|s| s.material.wave_amplitude.abs() > 1.0e-4);
```

Both terms are effectively constant-true in the shipping binary:
- `TotalTime` is inserted unconditionally at boot (`byroredux/src/boot.rs:375`), so `time_secs.is_some()`.
- `WaterMaterial::wave_amplitude` defaults to **0.05** (`crates/core/src/ecs/components/water.rs:347`), with real vanilla WATR authoring **0.1** (pinned by `crates/plugin/tests/parse_real_esm.rs:220` and `:1357`) — both three orders of magnitude above the `1.0e-4` threshold.

There is no code path in a water cell that leaves `waves_active` false.

## Evidence
The regression test that exists to prove the fast path works, `buoyant_body_sleeps_so_static_fast_path_re_engages` (`water.rs:1296`), builds a bare `World::new()` and never inserts `TotalTime`. `time_secs` is therefore `None`, `waves_active` is `false`, and the test exercises a configuration the binary never reaches. It uses `WaterMaterial::default()`, whose amplitude (0.05) would flip the gate the moment `TotalTime` were present.

This is the same "each half pins its own contract in isolation; nothing tests the composed path" shape as the two scale defects from 2026-08-16 (#3064 / #3065).

Verified at HEAD: `water.rs:484-487` and `:509` unchanged; `components/water.rs:347` still `wave_amplitude: 0.05`.

## Impact
Two, of different weight.

1. **Cost** — a fully settled water cell now pays the whole per-body scan every frame: `collect_water_surfaces` + `collect_water_current_volumes` + a `targets` `Vec` built by iterating **every** entity with `RapierHandles` (all static colliders included — the Skyrim-architecture census is ~416/cell interior and tens of thousands on a radius-12 exterior), with a `RigidBodyData` and `WaterContact` lookup each, plus `collider.compute_aabb()` for every body inside the union XZ footprint. That is precisely the work the fast path was added to avoid, and the exterior-freeze goal `docs/engine/watal.md` §0 states.
2. **A now-conditional invariant** — the `apply_buoyancy` docstring still asserts "The sim quiesces; buoyancy can't pin it awake." With waves live, the re-wake condition at `water.rs:679-684` (`b.is_sleeping() && (surface_y − center_y − prior_depth).abs() > 0.1`) re-wakes a settled float, and `woke_any` then re-arms `pw.wake()` (`water.rs:794`). At the 0.05–0.1 amplitudes vanilla authors, the per-frame crest delta stays under the 0.1 BU threshold so the sim still quiesces — but the guarantee is now an amplitude-dependent accident rather than a structural property, and any WATR authoring ≳0.25 amplitude pins the step loop awake for the whole cell.

## Related
#2871 (OPEN — the dry→wet wake swallowed above 60 fps; same wake discipline), PHYS-D6-2026-08-20-01, `docs/engine/watal.md` §0.

## Suggested fix
Decide which property is wanted and make it structural. Either:

(a) keep the wave-follow and drop the dead gate, replacing it with a cheaper reachable one — skip the scan unless `!surfaces.is_empty()` **and** some body already carries a `WaterContact` or is awake, so an all-dry settled cell still short-circuits; or

(b) keep the fast path and make wave-following event-driven — recompute `surface_y` only for bodies that already have a `WaterContact`, which is a bounded set, instead of re-scanning all bodies.

Either way, insert `TotalTime` into `buoyant_body_sleeps_so_static_fast_path_re_engages` so the test measures the shipping configuration, and restate the docstring's pinning claim with its real precondition.

Sequencing: do this **after** PHYS-D6-2026-08-20-01, whose fix changes the force bookkeeping this scan does.

## Completeness Checks
- [ ] **SIBLING**: Same "test builds a bare `World` that the binary never reaches" pattern checked in the other `crates/physics` fast-path tests
- [ ] **TESTS**: A regression test pins this specific fix — with `TotalTime` present and a non-zero authored amplitude
