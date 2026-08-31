# PHYS-2026-08-30-01: is_grounded flips true/false every frame on a stationary player — 58% of frames log a grounded [TRANSITION] and jump input is dropped on half of them

State: OPEN · Labels: bug high physics character 

Observed live on FO4 `BADTFL01` (`cargo run --release -- --game fo4 --cell BADTFL01`), but the mechanism is game-independent — it is pure `character_controller_system` arithmetic.

## Symptom

A player standing perfectly still alternates `is_grounded` true/false **every single frame**, forever. Out of 6103 frames in one session, **3533 (58%) logged a grounded `[TRANSITION]`**:

```
M28.5 frame 2484: body Y -188.0→-188.0 (Δ 0.001), v -30.2, grounded=false [TRANSITION]
M28.5 frame 2485: body Y -188.0→-188.0 (Δ 0.001), v -60.7, grounded=true  [TRANSITION]
M28.5 frame 2486: body Y -188.0→-188.0 (Δ 0.007), v -29.5, grounded=false [TRANSITION]
M28.5 frame 2487: body Y -188.0→-188.0 (Δ -0.008), v -59.3, grounded=true [TRANSITION]
M28.5 frame 2488: body Y -188.0→-188.0 (Δ 0.003), v -27.0, grounded=false [TRANSITION]
M28.5 frame 2489: body Y -188.0→-188.0 (Δ -0.003), v -54.5, grounded=true [TRANSITION]
```

Body Y is constant to within 0.01 BU across the whole window — nothing is actually moving. Only the *reported* ground contact oscillates.

## Mechanism — a two-frame limit cycle

Frame A (`controller.is_grounded == true`, `vertical_velocity == 0`):
1. `integrate_vertical` produces `v ≈ -24` (one gravity step) — this is the value logged.
2. Because `is_grounded` is true, `byroredux/src/systems/character.rs:335` takes the #2857 clamped-probe branch instead of `v * dt`. The capsule is already at exact resting contact, so `correction` at `character.rs:360` is ~0 and `desired_translation.y ≈ 0`.
3. Rapier's KCC is handed a ~zero vertical request, sweeps nothing, and returns **`grounded = false`**.
4. Writeback at `character.rs:468` therefore takes the `else` arm: `c.vertical_velocity = -24`, `c.is_grounded = false` (`character.rs:473`).

Frame B (`is_grounded == false`, `v == -24`):
1. `integrate_vertical` produces `v ≈ -48`.
2. `is_grounded` is false, so `desired_vertical = v * dt ≈ -0.9 BU`.
3. The KCC sweeps down, hits the floor, returns **`grounded = true`** with `translation.y ≈ 0`.
4. Writeback zeroes `vertical_velocity` and sets `is_grounded = true`.

→ back to frame A. The cycle is self-sustaining and never settles.

The root of it: #2857 correctly stopped the grounded probe from tunnelling through convex floors by clamping it to the real support gap, but a capsule already at resting contact then asks for *zero* motion — and a zero-length sweep is exactly the input for which the KCC cannot report contact. The state that keeps `is_grounded` true is thus unreachable two frames running.

## Impact

- **Jump is dropped on ~half of all frames.** `jump_fired` (`character.rs:271`) requires `controller.is_grounded`; a Space press landing on a frame-A tick is silently swallowed.
- Any consumer keyed on the grounded edge — footstep audio, landing/impact reactions, fall damage, locomotion animation state — sees a spurious land/leave pair every frame.
- `vertical_velocity` never rests at 0; it sawtooths between ~-25 and ~-60 on a body that is standing still.
- The M28.5 diagnostic is destroyed as a diagnostic: it fires on 58% of frames, so the "I fell into the void" / "I'm stuck in a wall" signals it exists to surface are buried in per-frame spam (3533 INFO lines in ~2.5 minutes).

## Suggested direction

The `grounded` bit should not be sourced solely from the KCC's response to a request the grounded branch deliberately makes zero-length. Options, roughly in order of preference:

1. When the clamped probe branch runs and `cast_capsule_down` **did** find a support surface within `step_height + offset`, treat that as authoritative ground contact and OR it into `result.grounded` — the probe already did the query, its answer is being thrown away.
2. Hysteresis on the state itself: only clear `is_grounded` after N consecutive airborne frames, so a single zero-sweep frame cannot flip it.
3. Keep a minimum probe length (a fraction of `kcc_offset`) so the sweep is never degenerate — riskier, this is the direction #2857 had to walk back.

Option 1 costs nothing extra and reuses a query already being paid for.

## Repro

```
cargo run --release -- --game fo4 --cell BADTFL01
```
Stand still. Every frame logs a `[TRANSITION]`. Any interior on any game should show it — nothing here is FO4-specific.

