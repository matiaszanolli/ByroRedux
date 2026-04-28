# INVESTIGATION — Issue #731

## Status: covered by #729's fix (commit 4f3b50f), no additional code changes required.

## What the user asked for vs. what landed

The ticket explicitly states "Add `fog_color` to the
`apply_worldspace_weather` log line. Resolves #729 and this ticket
simultaneously." Commit `4f3b50f` (Fix #729) already:

* Added `fog_color={:?}` to the bootstrap log
  (`byroredux/src/scene.rs:192`).
* Fixed the NAM0 group-index off-by-one for slots 2–9.

## On the user's `SKY_FOG = 1` hypothesis

The user wrote: `SKY_FOG` "likely doesn't match FNV's actual NAM0
layout". I verified against the tes5edit fopdoc (FNV + FO3 share the
same WTHR NAM0 schema):

```
0 Sky-Upper
1 Fog        ← parser correctly indexes here
2 Unused
3 Ambient
4 Sunlight
5 Sun
6 Stars
7 Sky-Lower
8 Horizon
9 Unused
```

`SKY_FOG = 1` is correct and was correct before #729. The off-by-one
only affected the constants for slots 2 onward (`SKY_AMBIENT` was 2
and is now 3, etc.). So the fog colour the renderer feeds to the
composite shader has always come from the documented Fog group. If
`NVWastelandClear`'s authored Fog colour is itself a desaturated
greyish blue, that is Bethesda's authored choice, not a parser bug.

## On the perceived ~30m view distance

Quick math against the composite fog blend
(`crates/renderer/shaders/composite.frag:264-275`):

```glsl
fogFactor = smoothstep(near, far, worldDist) * 0.7
```

`fog_near` is clamped to 0 in `byroredux/src/render.rs:972` (#666),
so the blend shape is `smoothstep(0, 200000, dist) * 0.7`. At ~30 m
(~2100 game units) the factor is ~0.007, not enough to dominate any
pixel. At ~300 m (~21000 units) it's ~0.07. So fog distance math
alone doesn't explain "everything past 30m looks the same colour."

Plausible alternative mechanisms (not investigated runtime here —
need a re-run with the #729 log to discriminate):

* Distance ambient saturation if RT GI / shadow signal is sparse.
* Cluster culling cutting light contribution past some short
  threshold.
* Sky depth-test (`is_sky = depth >= 0.9999`) inadvertently treating
  far-but-not-clipped terrain as sky if z-write or far-plane
  interaction skews near unity.

None of these are in scope for *this* ticket's stated mechanism.
The user explicitly framed this as a fog-colour discriminator
exercise; that exercise is now a one-line read of the next FNV
bootstrap log, which #729 already enables.

## What to do next session

Re-run the repro and read the new bootstrap log:

```
WTHR 'NVWastelandClear': zenith=[…] horizon=[…] sun=[…]
  ambient=[…] sunlight=[…] fog_color=[…] fog_day=…
```

* If `fog_color` is itself green / dull-grey → that is the authored
  slot-1 RGB. Cosmetic complaint about Bethesda authoring; not an
  engine bug.
* If `fog_color` is warm / matches Mojave aesthetic → fog is fine,
  the perceived "30m view distance" has a different root cause and
  needs its own ticket.

## Plan

Close as covered by #729. No new commit.
