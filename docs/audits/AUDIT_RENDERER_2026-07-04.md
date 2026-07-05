# Renderer Audit — 2026-07-04

Deep audit of the Vulkan deferred + ray-traced renderer across all 21 skill
dimensions (AS correctness, SSBO/RT ray-query plumbing, GPU-struct layout,
sync/barriers, GPU memory/lifecycle, NIFAL material translation, material table,
denoiser/composite, GPU skinning, camera-relative precision, pipeline/render
pass, command-buffer recording, TAA, caustics, water, volumetrics/bloom, Disney
BSDF/soft shadows, sky/weather, tangent-space, debug/telemetry, Cornell harness).

- **Branch**: main · **Prior baseline**: `AUDIT_RENDERER_2026-07-03.md` @ `8498e559`
  ("excellent condition", 0 CRITICAL/HIGH/MEDIUM, 2 pre-existing LOW).
- **Depth**: deep — delta-focused per-dimension, plus a targeted adversarial
  investigation into two live user-reported visual symptoms (see below).
- **Authoritative references**: `docs/engine/shader-pipeline.md`,
  `docs/engine/memory-budget.md`.
- **Dedup baseline**: `gh issue list` (open issues) + the 07-03 report.
- **Trigger**: two symptoms reported live from a running instance during this
  session — (1) abnormally "chrome"/mirror-like Fallout-interior decorative
  wall props ("flyers"), and (2) a ghosted/double-image translucency artifact
  in a Skyrim interior, offset diagonally, including a doubled view of the
  player's own body.

## Methodology note

Each of the 21 dimensions was run as an independent subagent, instructed to
treat the 07-03 report as a trusted baseline and focus fresh verification on
the code delta since its HEAD commit (`8498e559`) — roughly 50 commits, of
which ~24 touch renderer-adjacent paths (`crates/renderer/`,
`byroredux/src/render/`, `byroredux/src/asset_provider/material.rs`). Several
dimensions (14, 15, 17, 18, 19, 21) found **zero delta** in their own
entry-point files and served as pure regression re-confirmation passes.
Dimensions 2, 6, 8, 10, and 13 were additionally asked to investigate the two
live symptoms from their respective angles, cross-checking each other's
hypotheses.

## Executive Summary

| Severity | Count | IDs |
|---|---|---|
| CRITICAL | 0 | — |
| HIGH | 0 | — |
| MEDIUM | 1 | REN-2026-07-04-M01 (chrome-flyer PBR classifier gap, Dim 6) |
| LOW | 2 (re-confirmed, not new) | Existing #1860, #1861 |
| INFO / issue-hygiene | 1 | #1824 confirmed STALE — recommend closing |

The renderer remains in **excellent condition**. Every dimension either found
zero delta since the trusted 07-03 baseline, or confirmed the delta commits
(#1783/#1789/#1800/#1802/#1805/#1806/#1808/#1809/#1810/#1811/#1812/#1814/#1831/#1868)
are correct and well-tested. **One new MEDIUM finding** surfaced: a pre-existing
(not delta-introduced) gap in the CPU-side PBR classifier that is the confirmed,
symbol-verified root cause of the live "chrome flyer" symptom. The ghosting
symptom investigation converged on a plausible mechanism (a shared,
spatially-uniform bad motion vector, amplified into a stuck artifact by TAA's
intentional parked-camera clamp bypass) but the ultimate origin of the bad
motion vector could not be pinned down without a live RenderDoc capture — this
is reported as the audit's one **Needs-RenderDoc** item of substance.

## Symptom Investigations

### Symptom 1 — "Chrome" Fallout-interior flyers/posters: ROOT CAUSE FOUND

**Verdict: confirmed, reachable on real vanilla content, symbol-anchored, CPU-side, fully unit-testable.**

Dimension 6 (NIFAL material translation) traced the mechanism end-to-end:

1. A FO3/FNV decorative flyer/poster carries `BSShaderPPLightingProperty`,
   whose walker arm (`crates/nif/src/import/material/walker.rs:767`, setting
   `env_map_scale` at line 847) authors `env_map_scale ≈ 1.0` on "nearly every
   FNV surface" per the classifier's own source comment.
2. That same walker arm never touches `specular_color` — only the
   `BSLightingShaderProperty` arm (line 349, Skyrim+/FO4) or the
   `NiMaterialProperty` arm (line 635, Oblivion-era) do. A PPLighting mesh
   without a co-bound `NiMaterialProperty` — common for decorative FO3/FNV
   planes — leaves `specular_color` at the `MaterialInfo` struct default
   `[1.0, 1.0, 1.0]` (`crates/nif/src/import/material/mod.rs:961`).
3. The flyer's diffuse path contains no classifier keyword (`flyer`/`poster`/
   `note`/`paper` match none of the metal/wood/stone/glass/fabric arms in
   `classify_pbr_keyword`), so it falls to the `env_map_scale > 0.3` arm
   (`crates/core/src/ecs/components/material.rs:548`).
4. With `specular_color = [1,1,1]`: `spec_lum = 1.0` →
   `metalness = ((1.0−0.5)*0.8).clamp(0,0.4) = 0.4`;
   `roughness = min(0.8, 0.55) = 0.55` (the metallic-tier ceiling, since
   `spec_lum > 0.6`).
5. `metalness=0.4, roughness=0.55` crosses the RT-reflection gate
   (`roughness < 0.6`, `triangle.frag:1795`) → RT reflections engage on a
   metalness-0.4 flat paper surface → abnormally reflective/mirror-ish flyer.

Dimensions 2 and 17 independently cross-checked the shader side and confirmed
**no additional amplification**: the reflection weighting is plain
Fresnel-only (`F0 = mix(0.04, albedo, metalness)`, the old
`metalness>0.3 && roughness<0.6` double-jeopardy gate is already gone), and
since the mesh is legacy (no BGSM), the Disney diffuse lobe is never reached
either. Dimension 19 confirmed the tangent-space/normal-map path contributes
nothing (a texture-less flyer carries no normal map, so `perturbNormal` is
never entered). The severity and visual magnitude are exactly what Dimension
6's CPU-side trace predicts — nothing downstream makes it worse.

`PbrClassifierInputs` carries no "was specular actually authored?" signal, so
it cannot distinguish an authored-white specular from the struct default.
Every existing env-map-arm regression test uses a low specular `[0.2;3]`
fixture — none exercises the `[1,1,1]` default, which is the coverage hole
where this symptom lives.

**What's unconfirmed:** that a *specific* vanilla flyer NIF actually omits
`NiMaterialProperty` and authors `env_map_scale > 0.3` — those are content
assertions, not code facts, and would benefit from a `tex.missing`/live check
to fully close the loop, per this codebase's own established
"chrome → check textures/material data first" convention.

**Suggested direction (not applied by this audit — audits don't fix):**
thread a `specular_authored: bool` into `PbrClassifierInputs`; require it
before the env-map arm lifts metalness from `spec_lum`; when unauthored,
treat as dielectric (metalness 0.0), keep the matte roughness ceiling. Add the
missing `[1,1,1]`-default regression test. Self-contained CPU change,
verifiable by `cargo test -p byroredux-core` alone — no RenderDoc needed for
the fix itself, though a visual spot-check on real content is the natural
final confirmation.

### Symptom 2 — Ghosted diagonal double-image (Skyrim interior): MECHANISM NARROWED, ORIGIN UNCONFIRMED

**Verdict: inconclusive without a live capture — this audit's one substantive Needs-RenderDoc item.**

Four dimensions investigated this adversarially, each ruling parts in or out:

- **Dimension 2** (RT/ray-query) ruled out the RT reflection/refraction/glass
  paths as the origin (no path there produces a full-screen ghosted
  duplicate), and flagged the G-buffer Mesh-ID attachment's bit 31
  (`ALPHA_BLEND_NO_HISTORY`, the SVGF-accumulation-skip marker) as the most
  relevant lever for the denoiser/TAA dimensions to check.
- **Dimension 8** (denoiser/composite) confirmed the bit-31 write and decode
  are both correct end-to-end (not the cause), and produced the leading
  hypothesis: **H1 — a spatially-uniform wrong motion vector shared by both
  SVGF and TAA**, since the symptom (a *full-screen, uniform, diagonal* offset
  doubling both room geometry and the player's own body) is inconsistent with
  a per-object or per-material fault, and a uniform offset on large flat
  interior surfaces (uniform mesh-ID, uniform normal) would pass every
  disocclusion/normal-cone rejection test SVGF has.
- **Dimension 10** (camera-relative precision) traced Dimension 8's secondary
  hypothesis (H2 — a stale `prev_render_origin` from a skipped `draw_frame`
  early-out) and **definitively ruled it out**: the `prev_view_proj`/
  `prev_render_origin` pair is written at two adjacent unconditional
  statements with no branch between them, every early-out that could skip the
  pair-update also skips the frame-counter advance (so the pair and the
  presented frame stay coupled), and the origin-correction math applies the
  *full* delta regardless of how many frames were skipped — a stale pair is
  still self-consistent, not desynced.
- **Dimension 13** (TAA) confirmed TAA's own motion-vector read and
  reprojection math do not originate a diagonal offset, but identified
  **why an upstream offset would get stuck and persist**: TAA's parked-camera
  path (`pixelStatic`) deliberately skips the YCoCg luma clamp (by design,
  #1479, since that clamp is "actively harmful to convergence" when the
  camera parks) and drives the blend weight toward ~99% history. If a bad,
  spatially-uniform motion vector baked a doubled image into history during a
  brief transient (matching H1), parking the camera would freeze that ghost
  in place indefinitely with no mechanism left to reject it — exactly the
  "stuck ~50% diagonal double" character reported, and it would appear in
  TAA's own output for direct-lit geometry (the player's body), which is
  Dimension 8's stated confirmation criterion for H1 over an SVGF-only
  explanation.

**Net conclusion:** the most plausible chain is H1 (a transient,
spatially-uniform bad motion vector shared by SVGF and TAA) + the TAA
parked-camera clamp bypass (an intentional, tested, unrelated feature) acting
as an amplifier that locks the artifact in place once the camera stops
moving. **The origin of the bad motion vector itself was not found** — every
motion-vector-authoring site traced (`triangle.vert`'s clip-position emission,
`triangle.frag`'s `outMotion` write, the CPU-side `origin_corrected_prev_view_proj`)
reads correct on static analysis. This is squarely a case where the failure
mode is invisible to `cargo test` and needs a live capture.

**What a RenderDoc capture would need (per the no-speculative-fix policy — no
shader/barrier change is proposed by this audit):**
1. The motion-vector G-buffer on an affected frame — is the offset uniform
   across the whole screen (supports the camera-level hypothesis) or
   localized to the body (would redirect to skinning)?
2. `prevViewProj` in the CameraUBO vs. the actual previous frame's `viewProj`,
   and both `render_origin` values, on that frame.
3. SVGF history + `moments.b` (`histAge`) — is the ghost baked into
   accumulated history, and is `histAge`/the blend weight sitting in the
   sticky ~50% regime the symptom's "50% opacity" description matches?
4. TAA history in the same capture — confirming the doubled direct-lit
   geometry appears there too would close the loop on H1 over an
   SVGF-indirect-only explanation.

## RT Pipeline Assessment

**BLAS/TLAS (Dim 1)** — clean. The only fresh delta touching this dimension
(#1789 comment re-citation, #1812 redundant-refit skip) is correct and
well-guarded. All CRITICAL invariants (`instance_custom_index == SSBO index`,
BUILD-vs-UPDATE decision, deferred BLAS destruction) hold unchanged.

**SSBO/RT ray queries (Dim 2)** — clean. `instance_custom_index` indexing,
shadow/reflection ray tMin+bias, glass/IOR refraction (Frisvad basis,
window-portal demote, ray-budget cap), ReSTIR-DI spatial reuse (25° normal
cone), and the BC1 punch-through alpha pin are all intact. The only shader
delta (#1800, GI light sort) is comment-only for this dimension.

**Denoiser/composite (Dim 8)** — clean on the delta; the two skinning-skip
commits (#1811/#1812) provably cannot inject a spurious motion vector. See
Symptom 2 above for the one open item.

**Camera-relative precision (Dim 10)** — clean; the two-convention boundary
(raster relative / RT absolute), derivative-varying split (#1496), and RT
absolute ceiling (`2^20`, #1495) are all intact and unchanged since baseline.

## GPU-Struct & Memory Assessment

**GPU-struct layout (Dim 3)** — clean; zero delta to any `#[repr(C)]`/GLSL
struct, flag constant, or capacity constant. All size/offset pins hold.

**GPU memory & lifecycle (Dim 5)** — clean; the deferred BLAS-scratch destroy
(#1782), TLAS resize wait-idle (#1390), and `AllocatorResource` teardown
ordering (#1406/#1477) all hold. The one re-verified open issue (#1861,
command-buffer/fence leak on Vulkan API error paths) remains accurate and is
correctly a LOW, not a per-frame leak.

**Material table (Dim 7)** — clean; zero delta touches the dedup path. Hash/Eq
byte-invariant, over-cap handling, and the particle color-fade quantization
guard (#1795) all hold.

**GPU skinning (Dim 9)** — the largest concentration of fresh commits in the
whole renderer (#1811, #1812, #1783, plus a re-verification of the
pre-baseline #1794 bone_world elimination). All confirmed correct, including
two adversarial cross-checks: #1794's stale-slot-tail data is provably inert
(bounded by each mesh's own bone count at import time), and #1811+#1812's two
independent skip-guards were proven unable to compound into a wrongly-skipped
dirty entity (they're mutually exclusive per-frame by construction).

## Findings

### MEDIUM

**REN-2026-07-04-M01** — `classify_pbr_keyword`'s env-map-scale arm cannot
distinguish an authored-white specular from its `[1,1,1]` struct default,
producing non-trivial metalness (0.4) + sub-gate roughness (0.55) on
property-sparse legacy meshes. Confirmed root cause of the live chrome-flyer
symptom (see Symptom 1 above). Location: `classify_pbr_keyword`
(`crates/core/src/ecs/components/material.rs:548`), fed by
`MaterialInfo::classify_legacy_pbr` (`crates/nif/src/import/material/mod.rs:1040`).
Not delta-introduced — pre-existing gap the trusted baseline's checklist
didn't probe for this specific input combination.

### LOW (re-confirmed, not new)

- **#1860** — `DBG_BITS` test catalog covers only 13 of 17 live `DBG_*`
  constants; the 4 uncovered bits (`DISABLE_MULTISCATTER`/`_ATROUS`/`_RESTIR`/
  `_SPATIAL`) are actively read by shaders but unpinned by the completeness
  tests. Re-verified accurate against current code (Dim 20).
- **#1861** — `with_one_time_commands_inner` leaks the command buffer (and on
  the owned-fence path, the fence) on Vulkan API error paths other than the
  recording-closure failure. Re-verified accurate; still correctly scoped as
  LOW (triggers only on device-lost/OOM/driver-fault, not a per-frame leak)
  (Dim 4, Dim 5).

### Issue hygiene

- **#1824 — STALE, recommend closing.** Describes `gl_to_gamebryo_blend`
  truncating `u32→u8` with no range guard. That function was renamed to
  `bgsm_blend_to_gamebryo` and reduced to a bare identity narrowing in #1823
  (commit `27334481`), which also removed the wrong blend-factor swap. The
  cast remains but is the entire documented-identity function, pinned by
  `bgsm_blend_to_gamebryo_is_identity_narrowing` over the full real value set
  (the reference parser only ever emits `{0,1,4,6,7}`). The residual "no range
  guard" concern is moot: the actual consumer, `gamebryo_to_vk_blend_factor`
  (`crates/renderer/src/vulkan/pipeline.rs:162`), has a `_ => SRC_ALPHA`
  fallback arm, tested by `gamebryo_to_vk_blend_factor_covers_all_11_values`.
  An out-of-range Gamebryo enum cannot produce invalid pipeline state.
  (Dim 6.)
- **#1857 — still accurate, re-confirm open.** `context/draw.rs` is now
  **4584 LOC** (was 4265 when filed) — grown, not shrunk, since. (Dim 12.)

## Per-dimension summary (all 21)

| Dim | Area | Delta since 07-03 baseline | Result |
|---|---|---|---|
| 1 | AS/BLAS-TLAS | #1789, #1812 | Clean |
| 2 | SSBO/RT ray queries | comment-only (#1800) | Clean; symptom-1 hypothesis ruled out for this dimension |
| 3 | GPU-struct layout | none | Clean |
| 4 | Sync & barriers | #1811, #1812 | Clean; #1861 re-confirmed |
| 5 | GPU memory/lifecycle | #1782 (ancestor), #1868 (comments) | Clean; #1861 re-confirmed |
| 6 | NIFAL material translation | #1823, #1831 | **1 MEDIUM finding** (chrome-flyer root cause); #1824 stale |
| 7 | Material table (R1 dedup) | none in-scope | Clean |
| 8 | Denoiser & composite | #1811, #1812 | Clean; symptom-2 H1 hypothesis + H2 raised |
| 9 | GPU skinning + BLAS refit | #1811, #1812, #1783 | Clean, adversarially cross-checked |
| 10 | Camera-relative precision | none | Clean; symptom-2 H2 definitively ruled out |
| 11 | Pipeline state/render pass | none (sort-key only, #1806) | Clean |
| 12 | Command buffer recording | #1805, #1806 | Clean; #1857 re-confirmed (grown) |
| 13 | TAA | none | Clean; symptom-2 mechanism (not origin) explained |
| 14 | Caustic splat | none | Clean |
| 15 | Water + water-caustics | none | Clean |
| 16 | Volumetrics & bloom | comment-only | Clean |
| 17 | Disney BSDF / soft shadows | comment-only (#1800) | Clean; symptom-1 shader cross-check confirms no amplification |
| 18 | Sky/weather/exterior | none | Clean |
| 19 | Tangent-space/normal maps | none | Clean; symptom-1 sanity check confirms no amplification |
| 20 | Debug overlay/telemetry | comment-only | Clean; #1860 re-confirmed |
| 21 | Cornell-box RT harness | none | Clean |

## Prioritized Fix Order

1. **REN-2026-07-04-M01** (chrome-flyer PBR classifier gap) — self-contained
   CPU fix, fully unit-testable, no RenderDoc needed. Highest-value next
   action: thread a `specular_authored` signal into the classifier.
2. **Symptom 2 (ghosting)** — needs a RenderDoc capture before any fix can be
   proposed responsibly; do not guess at a shader/barrier change.
3. **Issue hygiene**: close #1824 (stale); no code change required.
4. Pre-existing LOWs (#1860, #1861) — unchanged priority, still open, still
   correctly scoped as non-urgent.

## Needs-RenderDoc

- **Symptom 2 root cause** (see above) — the single substantive item from
  this audit requiring live GPU capture. Four dimensions (2, 8, 10, 13)
  converged on where to look; none could close the loop from static analysis
  alone.
- Routine per-dimension "needs RenderDoc" notes carried forward from the
  trusted baseline (AS-build input barrier hazard count under `BYRO_VALIDATION`,
  G-buffer→compute-consumer layout transitions) — unchanged, no new concern.

## Disproved / Not Reported

- Dimension 6's initial structural hypothesis that `env_map_scale` itself
  defaults unsafely high was checked and refuted — `MaterialInfo::env_map_scale`
  defaults to `0.0`; the metallic-arm only fires when a shader property
  genuinely authors it above `0.3`, which is common but not universal.
- The pre-existing #1462 volumetric froxel-depth convention mismatch remains
  correctly deferred behind `VOLUMETRIC_OUTPUT_CONSUMED=false` (Dim 16) — not
  re-reported.
- Cornell-harness metalness-vs-lighting confound and glass-stipple/IGN
  refraction jitter remain documented observations, not bugs (Dim 21) — not
  re-reported.
</content>
