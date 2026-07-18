# #2013 investigation — TES-family character grounding failure (live session)

## Summary

Unlike the prior code-review-only pass (`.claude/issues/2012 2013 2014 2015/INVESTIGATION-2013.md`),
this session had a live Vulkan device + real game data available, so the
recommended empirical checks from that prior investigation were actually run:
`--game oblivion --cell ICMarketDistrictTheGildedCarafe --bench-frames 240
--bench-hold`, with a temporary diagnostic instrumented into
`character_controller_system` (reverted before this commit — nothing below
shipped as code).

**No safe, verified code fix was found.** The investigation narrowed the
mechanism substantially and ruled out two of the three hypotheses the prior
pass left open, but the final piece — why Rapier's `KinematicCharacterController`
never registers a contact manifold for the resting position it itself found —
sits inside `rapier3d`/`parry3d`'s internal contact-generation algorithm, which
requires deeper native instrumentation (patching a local rapier3d copy, or an
isolated minimal repro outside this engine) to nail down conclusively. Per
project discipline (`feedback_speculative_vulkan_fixes` — don't ship changes
when failure modes are invisible to `cargo test`; verify or don't ship), no
code change was made. **Issue remains open.**

## Reproduction confirmed

`M28.5 frame N: body Y a→b …` log lines reproduce the issue's own evidence
almost exactly: falls from Y=414.8 to ~Y=324 by frame ~60–120, then freezes
(`Δ≈0.000`) with `v` pinned at the -2000 terminal-velocity cap and
`grounded=false` forever after (verified out to 5000+ frames).

## Methodology bug caught and fixed mid-session

The first diagnostic (`PhysicsWorld::cast_ray_down_with_normal`, temporary)
used `QueryFilter::exclude_dynamic()` to skip the player's own collider —
**wrong**: the player capsule is `RigidBodyType::KinematicPositionBased`, not
`Dynamic`, so `exclude_dynamic()` doesn't exclude it. A ray cast from the
capsule's own center with `solid = true` self-hits at `time_of_impact = 0`.
This produced an identical, meaningless `drop=0.00, shape=Capsule` result on
**both** the broken Oblivion case and a known-good FNV baseline — a red
herring that would have been very easy to misread as a real finding. Fixed by
also passing `.exclude_collider(player_collider_handle)`. Flagging this
prominently in case a future session repeats the same probe.

## Hypotheses from the prior pass, now resolved

1. **Z-up→Y-up conversion inverts winding** — prior pass already ruled this
   out on paper (proper rotation, det=+1). **Confirmed empirically**: a
   corrected ray-cast straight down from the resting capsule center reports
   `normal=(0.038, 0.994, -0.098)`, `dot(normal, +Y) = 0.9944` — objectively
   correct, up-facing. Not the cause.

2. **Door-teleport spawns into an unloaded exterior (no floor at all)** —
   **ruled out for Oblivion**: `M28.5 static collider AABB` at frame 0 shows
   70 fixed colliders spanning x[-837,1471.5] / y[-448,565.1] / z[-1370.8,434.4],
   and the character spawns at (285.0, 414.8, -320.5) — well inside that
   volume. There IS solid architecture under the spawn point. (Not checked
   for Skyrim in this session — see Open questions below.)

## New finding: the character rests *above* its floor, not embedded in it — opposite of the working case

Excluding the self-hit correctly, a straight-down ray from the settled
capsule center (Y=323.99) found the room's main floor at **Y=245.48**
(shape=TriMesh, tilted-but-valid normal). Capsule bottom = center −
(half_height + radius) = 323.99 − 64 = 259.99. That's **14.5 BU *above*** the
floor — a real air gap, not a resting contact.

The FNV control (`GSProspectorSaloonInterior`, grounds at frame 0) shows the
**opposite** relationship: capsule bottom (3446.0) sits **~22 BU *inside***
its floor (3467.99) — i.e. genuinely overlapping/interpenetrating, which
trivially satisfies Rapier's contact-manifold distance check
(`contact.dist <= prediction`, prediction ≈ `kcc_offset_bu(4.0) × 1.1 = 4.4`
BU) since overlap always registers `dist < 0`.

Oblivion's 14.5 BU air gap is **more than 3× the ~4.4 BU prediction margin**
the KCC's `is_grounded_at_contact_manifold` search uses — this is the
proximate mechanical reason `grounded` never flips true, even though the
collide-and-slide TOI sweep (which searches the *entire* remaining per-frame
fall distance, not just the tiny prediction shell) correctly stops the body
from falling further.

Frame-by-frame logging of this gap during the fall (frames 40–90) shows a
smooth, monotonically-decreasing approach (69 → 55 → 40 → 24 → 14.5) that
then abruptly and completely freezes at 14.5 — not a gradual asymptotic
settle at the intended `kcc_offset_bu` (4.0) distance. Reproduced across two
separate runs (different frame-to-simulated-time mappings due to real
wall-clock dt jitter) landing at the **same** 14.512/14.513 gap both times —
this is a deterministic geometric/algorithmic property of this specific
spawn location, not measurement noise.

## New finding: a closer, different surface exists off-axis (unconfirmed as the actual contact)

The capsule has `radius = 18` BU, so contacts anywhwere within that lateral
distance of the center axis count, not just directly underneath. Probing
±18 BU in all four horizontal directions from the settled position (frame
300) found three directions hitting the same distant, tilted floor (~244–246,
matching the center probe), but the **+Z direction hit a different, closer,
perfectly-flat surface at Y=263.13** (`normal=(0,1,0)` exactly) — only ~3 BU
above the capsule's resting bottom (259.99), i.e. plausibly the *actual*
resting contact, versus the far-away (14.5 BU gap) main floor the center ray
happens to see past/around.

This is consistent with the player spawning at/near a door threshold (per
the existing "interior spawn = the cell's first door's own placement"
convention) where a raised sill, step, or small clutter object sits partially
under the capsule's footprint. If the capsule is resting on a small or
edge/corner contact from this nearby feature rather than a flat central
contact, that could plausibly explain why Rapier's persistent-manifold
generation fails to register it as `grounded` even though a geometrically
valid, correctly-oriented contact objectively exists nearby — parry's
contact-manifold generation for partial/edge contacts against composite
shapes is a known area of subtlety, but this wasn't verified further; the
"only 3 of 4 directions matched" data point was captured at the settled frame
but not otherwise cross-checked against the specific mesh instance.

## Why no fix shipped

Every remaining avenue from here is genuinely third-party-library territory
(`rapier3d::control::KinematicCharacterController::detect_grounded_status_and_apply_friction`
/ `is_grounded_at_contact_manifold`, and `parry3d`'s underlying
`contact_manifolds` dispatcher) or requires identifying the exact mesh/object
at Y≈263 near the spawn XZ — neither is something this session could safely
resolve without either patching a local rapier3d/parry3d copy for deeper
tracing, or writing an isolated minimal repro outside full game content.
Blindly tweaking `kcc_offset_bu`/`snap_to_ground`/prediction margins now,
without understanding why the gap settles at exactly ~14.5 BU instead of the
intended ~4.0 BU offset, risks masking this specific symptom while changing
behavior for every other game's character controller.

## Recommended next steps

1. Identify the specific static-architecture piece whose top surface sits at
   Y≈263 near `ICMarketDistrictTheGildedCarafe`'s spawn XZ (285, -320.5) —
   likely the spawn door's own threshold/sill geometry, given the existing
   "spawn at the door's placement" convention. Confirm whether the player is
   landing partially on it rather than the main floor.
2. If confirmed, this may be a spawn-*placement* issue (nudge the spawn point
   fully clear of door-threshold geometry) rather than a KCC/physics
   config issue — much lower risk to fix than tuning Rapier's contact
   generation.
3. Independently, re-run the same probe on `WhiterunDragonsreach` (Skyrim) —
   this session only investigated Oblivion in depth; the issue's own evidence
   describes a *different* sub-symptom for Skyrim (true infinite fall, never
   touches anything) which may have an entirely separate root cause (e.g. the
   door-teleport-to-unloaded-exterior lead, not yet ruled out for Skyrim
   specifically).
4. If (1)/(2) doesn't pan out, the next tool needed is native rapier3d/parry3d
   instrumentation (a local patched build) to log the actual candidate contact
   manifolds `detect_grounded_status_and_apply_friction` considers and why
   each is rejected — this investigation's ray-cast probes can confirm
   geometry exists but can't see what Rapier's own algorithm does with it.
