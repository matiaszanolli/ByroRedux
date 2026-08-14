# #2783 #2784 #2785 #2786 — renderer audit closeout (2026-08-14)

Four LOW findings from `AUDIT_RENDERER_2026-08-12b.md`. All in
`byroredux-renderer`; two touch `water.frag` and needed a SPIR-V recompile.

## #2783 (REN-D4-02) — per-image `render_finished` had no test
The contract from `548c1b69` was prose across six mentions, on a rule that had
already been **reverted once**: #906 moved these semaphores to
per-frame-in-flight, that tripped
`VUID-vkQueueSubmit-pSignalSemaphores-00067` on Skyrim Riverwood (3 swapchain
images > 2 frames in flight, FIFO), and `548c1b69` moved it back. Nothing would
catch a third swap.

Semaphore creation needs a device, so the test pins the two halves decidable
from source — the *count* (both create + resize loops size from
`swapchain_image_count`) and the *index* (`render_finished[img]`, never
`[frame]`) — plus a value test keeping the two counts conceptually apart.
**Verified it bites**: regressing one loop to `MAX_FRAMES_IN_FLIGHT` fails it.

Needles searched in `sync.rs` are composed at runtime; a literal would have
matched the test's own source and stayed green after the code it guards was
deleted.

## #2784 (REN-D15-03) — inclusive uv guard, off by one
`lessThanEqual(uv01, vec2(1.0))` admits `uv01 == 1.0`, giving
`pixel == screen` — one past the last texel. Now rejects on the integer pixel
against the size, matching `caustic_splat.comp`. Removes the dependence on
Vulkan's discard-out-of-range rule, which was also the only thing keeping the
wholesale conversion in bounds against the 1×1 `placeholder_caustic_sink`.

## #2785 (REN-D15-04) — `fog_near` uploaded but never read
`t` was `hitDist / fog_far`, so one hard-coded curve served every water body in
every game.

**The obvious fix was wrong.** Both `WaterMaterial::fog_near` and
`WaterParams::fog_near` documented it as "the distance at which the shallow
colour reaches 50% mix", which suggests `exp2(-d/fog_near)`. Measured against
vanilla:

| plugin | WATR | note |
|---|---|---|
| Skyrim.esm | 34 | `fog_near = 0` for most (`MarkarthWater` 0/110, `BlackreachWater` 0/290, `HorseTroughWater01` 220/4710) |
| FalloutNV.esm | 78 | 0 for most (`NVCleanWaterGS` 7/58) |
| Oblivion.esm | 23 | median `fog_near/fog_far` = 0.001 |

A half-distance of 0 makes water opaque on contact. It is the **near plane of a
linear ramp** — the same pair semantics the cell-lighting fog already uses. So
`t = (hitDist - fog_near) / (fog_far - fog_near)`, which is **bit-identical to
the old curve wherever `fog_near == 0`** (the vanilla majority) and only returns
the authored clear margin to bodies that ask for one. That compatibility claim
is the regression test. Both docstrings corrected.

Costs no push-constant space — the value was already in `shallow.a` — so the
128-byte ceiling is untouched and `misc.w` (a literal `0.0`, genuinely
reserved) remains the one free slot.

## #2786 (REN-D4-03) — stale `in_dep` comment
Described composite as the upstream swapchain writer with `dstStage = NONE`.
Since the FSR tail that is `PresentationPipeline`, whose outgoing dep #2143 gave
`COLOR_ATTACHMENT_OUTPUT | TRANSFER`. Comment corrected, and since the two
halves of that edge live in different files and drifted once already, added
source-level pins tying them together.

## Shader compilation
`glslangValidator -V water.frag -o water.frag.spv` (plain `-V`, per CLAUDE.md).
Confirmed by rebuilding the **unmodified** source and getting a byte-identical
match to the shipped `.spv` — an earlier attempt with `--target-env vulkan1.2`
produced a different (94 KB vs 113 KB) binary and was discarded. Post-change:
113 800 → 114 240 bytes.
