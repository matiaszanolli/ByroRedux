P5 soak: intermittent grounded=false after the 10th F5→door→F9 cycle — body pinned at the restored pose with clamped fall velocity
STATE: OPEN  LABELS: 
Found by the new P5 soak gate (`docs/smoke-tests/p5-soak.sh`, FNV GSProspectorSaloonInterior reference route) on 2026-10-01, first 30-minute run.

## Signature

Cycle 10's `input.press quickload` restored the body to **exactly** the quicksaved pose, but `grounded` stayed `false` for the full 300 s wait while `vertical_velocity` sat at the `-2000` terminal clamp and the position never moved:

```
# F5-time pose (grounded when saved), cycle 10:
body=(107.73, 3525.68, 856.52) grounded=true  vertical_velocity=0.00
# after F9 restore, final poll of 300 s:
body=(107.73, 3525.68, 856.52) grounded=false vertical_velocity=-2000.00
```

The restore itself completed (`save load: restored player pose at (107.7, 3525.7, 856.5)`), the engine kept ticking (scheduler logs continued), and the debug server kept answering `player.status` — so this is **not** a lost process or a stuck transition: the capsule rests at the restore pose but both ground-contact authorities (`result.grounded` from `move_character` and the `cast_capsule_down_surface_and_normal` walkable probe, `byroredux/src/systems/character.rs` ~L415-524) read no support.

## Repro rate

- 30-min soak run 1: FAILED at cycle 10 (`/tmp/byro-p5-soak.oNbNbm`, samples + full logs retained).
- 7-min soak rerun: PASS, 27 cycles incl. two ring wraps (`/tmp/byro-p5-soak.IKMQ1I`).
- Fast hammer (F5→activate→F9 without the walk segments, constant restore pose): 15/15 OK.

≈1 failure in ~52 restore cycles; the failing ingredient appears to need the per-cycle walk segments (each cycle restores to a *different* walk-back pose) or the slower cadence.

## Hypotheses (unchecked)

1. Session-replacement teardown leaves a stale/duplicate collider under the restored capsule; the KCC's excluded-collider handle and the probe disagree with the live set, so both miss the floor while depenetration pins the position.
2. The walk-back endpoint lands on marginal geometry (porch threshold/steps near the saloon door, z≈856); restoring the capsule half-embedded on a slope/step edge puts `result.grounded=false` and the walkable-normal probe under threshold simultaneously.
3. A race between the exterior streaming worker teardown and the interior rebuild's collider registration, hit only at the slower soak cadence.

## Evidence retained

`/tmp/byro-p5-soak.oNbNbm/`: `session.stderr` (10 restore lines, transition + loading-screen lifecycle), `c10.*` (F5 pose, arrival, restore poll), `soak.csv` (RSS curve — memory was bounded, ~2.6-2.8 GiB, so this is not the memory detector firing).

The soak gate now also captures per-second `player.status` samples and a `phys.stats`/`phys.census` pair on this specific failure, so a recurrence will carry the physics census that this first occurrence lacked.
