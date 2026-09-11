# Bundle: #3955 #3956 #3957 #3958

Two doc-rot findings and two EXAL-boundary leaks. All four premises verified at
HEAD before any edit; all four hold as written.

| # | Finding | Outcome |
|---|---|---|
| 3955 | approach system's doc bound to the scratch struct | doc moved |
| 3956 | FO4/FO76 authored height fog dropped at EXAL | carried; **both** sinks wired |
| 3957 | WTHR fog falloff power dropped | forwarded; `fog_clip` documented |
| 3958 | nifal.md §2 missing the #3549 skinning slice | sub-entry added |

## The census #3956 asked for

The issue rated itself MEDIUM on the boundary defect and said outright that
occupancy was **not** measured — "Census FO4/FO76 `FNAM` sizes before sizing the
work." Done, with a direct byte-level WTHR walk rather than the full parser
(whole-ESM parsing in `byroredux-plugin` has OOM-killed sessions before):

```
SeventySix.esm     121 WTHR   121 x 72B                      100%
Fallout4.esm        71 WTHR    63 x 72B, 6 x 32B, 2 x 56B     89%
DLCCoast.esm        16 WTHR    16 x 72B                      100%
DLCNukaWorld.esm    17 WTHR    17 x 72B                      100%
Skyrim.esm          84 WTHR    84 x 32B    (control — none, so the gate is right)
```

**217 of 225 FO4+FO76 weathers ship the tail.** Not an edge case, and the
magnitude is larger than the issue implies: median authored
`day_near_height_range` is 10000 units = **143 m** at 70 u/m, against the **30 m**
the engine used for all of them. The shipped altitude profile was ~4.8x too thin
wherever it was authored.

Incidental finding: **2 FO4 weathers ship a 56-byte `FNAM`** — an intermediate
form version neither the parser's `>= 72` gate nor the issue accounts for. The
gate declines them, which is the correct behaviour (no misparse), so nothing was
changed; noted here because the next person to touch this schema will meet it.

## The one thing not determinable, and what was done about it

The shader integrates `sigma_t(y) = sigma0 * exp(-(y - y0)/H)`. The authored
side is a `height_mid` + `height_range` pair. **No reference available to this
project says whether `range` is an e-folding height, a full-width-to-zero, or
something else** — Gamebryo 2.3 predates the field; nifxml and OpenMW do not
cover WTHR. Those readings differ by ~1.4x on a visual quantity `cargo test`
cannot check.

Per the No-Guessing policy this was raised rather than assumed, and the user
chose the identity (`H = range`): it introduces no invented constant, and it is
isolated in `FogMedium::with_authored_height_range` so a future reference
changes one expression.

`height_mid` is deliberately unconsumed — it anchors the profile (median 64
units, i.e. ground level) and the shader already anchors to *measured* ground
near the camera via `camera_pos.w` (#2225), which is strictly better than a
per-weather authored constant.

## Two places the work went beyond the issues

- **#3956 names one sink; there are two.** `post_passes.rs`'s froxel grid used
  the same constant. The composite tail deliberately continues from the grid's
  own boundary radiance to make the two agree at the seam, so feeding them
  different scale heights would make them disagree about altitude — worse than
  both being uniformly wrong. Both wired, on the user's call.
- **#3957's SIBLING check resolved cleanly.** `fog_clip` sits on the same line
  group, but WTHR has no clip-distance field at all (XCLL-only), so it stays
  `None` for a real reason — now stated, which is what the issue asked for
  under either outcome.

## Design note: why the height rides `FogMedium`

The issue suggested carrying `WeatherHeightFog` onto `WeatherDataRes` /
`CellLightingRes` and TOD-lerping it "the way `skyrim_dalc_per_tod` already is".
Putting the scalar on `FogMedium` instead is strictly less machinery: the TOD
blend already lerps `fog_media[0]` against `[1]`, so the height interpolates
with everything else and there is no second interpolator to keep in step.

The no-authored-data fallback resolves at the boundary, not as a draw-time
branch — the EXAL shape the issue asked for. That makes the canonical default
and the renderer constant the same quantity in two places, which is why
`the_default_scale_height_matches_the_renderer_constant` exists: if they drift,
every unauthored weather changes profile silently and everything still compiles.

## Caught by the repo's own gates

`swept_sources_carry_no_bare_line_number_anchors` (#2922) rejected a
`composite.frag:978` anchor in a new comment — line numbers rot, name the symbol.
Fixed there and in `env_translate.rs` (not swept, but the rule holds anywhere).
