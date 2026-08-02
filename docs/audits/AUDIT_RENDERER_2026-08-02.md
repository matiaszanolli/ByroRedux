# Renderer Audit — 2026-08-02

Scope: full 23-dimension `/audit-renderer` sweep. Session 62 (2026-07-26→2026-08-01,
`5f0220eb..7e068c7d`, 61 commits) landed the renderer's biggest single-session
feature push since FSR — procedural volumetric fog + clustered local fog
volumes, a new `MATERIAL_KIND_FIRE_REFRACTION` material kind, POM + a shared
secondary-ray tangent-frame include, structural shadow-mask/light-visibility
work, material/cubemap texture handling, and two water RT fixes — on top of
which `/session-close` separately fixed 4 stale test assertions the same day.
This audit runs about a week after the prior full sweep
(`docs/audits/AUDIT_RENDERER_2026-07-28.md`), which was itself mid-session and
did not see most of the above.

Each of the 23 dimensions ran as an independent agent with its own read of the
code; findings below are merged and de-duplicated across dimensions. Full
per-dimension detail (evidence, exact call chains) lives in the individual
dimension write-ups referenced by ID — this document is the synthesis.

## Executive Summary

**5 HIGH findings** (2 new, 1 existing-untouched, 2 more folded from a
recurring pattern), **~33 MEDIUM findings**, **~55 LOW findings /
regression-guard confirmations**, **0 CRITICAL**. Two previously-open issues
are **CONFIRMED-FIXED** this sweep. Two `MATERIAL_KIND_FIRE_REFRACTION`
consistency defects and two Cornell-harness coverage gaps recur across
multiple dimensions — see "Cross-Cutting Themes" below.

**Confirmed fixed:**
- **REN-2026-07-28-01** (composite.frag caustic decode dropped by SPIR-V
  drift) — restored by `4d7abd28`; source/binary agreement re-verified via
  `scripts/check-shader-artifacts.sh` (all 21 shaders match). Confirmed
  independently by Dimensions 8 and 15.
- **#2220** (authored CELL/WTHR fog uploaded but consumed by no shader) — the
  new volumetric fog work genuinely consumes extinction, chromaticity, peak
  radiance, and coverage. The 2026-07-28 audit's "authored medium integration
  is missing" verdict is stale; recommend closing #2220 and updating that
  report's Dimension 16 row.

**Untouched, still open:**
- **#2218** (FO3 Megaton exterior whiteout, HIGH) — verified Session 62's
  lighting/shadow work does not reach the FO3 exterior path (no DALC on
  FO3/FNV/Oblivion). Needs RenderDoc as the issue already states.
- **#2215** (indirect-draw grouping regression) — two Session 62 changes
  (`#2165`'s stricter 7-tuple batch key, `24e5cb6a`'s composition-phase
  partition) both push toward *more* indirect calls, consistent with the
  reported symptom. Worth adding to the issue before further bisection.
- **#2219** (skinned RT secondary-hit normals from bind-pose vertices) —
  Session 62's `ray_hit.glsl` consolidation widened its blast radius from
  normals to the full tangent frame (used by POM too). Scope note added to
  the tracking, not a new issue.

**New HIGH findings (2):**
1. **Water-side caustics refract through the flat geometric plane normal, not
   the wave-perturbed one** (REN-D15-01) — structurally cannot produce a
   caustic pattern; the code comment claims the opposite of what it does.
2. **Fire-refraction proxies remain `SHADOW_MASK_OPAQUE` occluders** despite a
   shader comment claiming TLAS exclusion (REN-D2-01) — heat-haze planes cast
   shadows on the light they're meant to be transparent to.
3. **Volumetric height-fog's reference altitude is the camera's own Y**, not
   a world datum (REN-D16-01) — density follows the player and breaks the new
   temporal-reprojection contract on vertical motion.
4. **A stale exterior `SkyParamsRes` survives an exterior→interior
   transition** except for its ambient cube, which Session 62 fixed — every
   *other* field, including `is_exterior` and the whole TOD sky, leaks into
   interiors with an unsealed roof/mesh gap (REN-D18-01).

## Cross-Cutting Themes

**The `is_sky` early-return branch in `composite.frag` skips more than tone-
mapping.** Two independent dimensions (8 and 16) found that bloom and the
volumetric/height-fog term are both applied only in the geometry (`has_surface`)
branch — sky pixels get neither. Same root branch structure, two symptoms
(REN-D8-02, REN-D16-02); fix both in one pass through that function.

**`MATERIAL_KIND_FIRE_REFRACTION` (103, new this session) has four
independent consistency gaps**, none of them silent-corruption but all real:
it stays in the TLAS as a shadow-ray occluder despite a comment claiming
otherwise (Dim 2), it overwrites the opaque receiver's G-buffer normal/motion
records at any coverage (Dim 11), its composition-phase sort key inverts
back-to-front order against unrelated alpha-over transparents behind it (Dim
12), and — separately, not a bug but a coverage gap — the one RT reference
harness built for exactly this kind of feature (Cornell) has no way to
exercise it at all (Dim 21: `mat.set` can't reach `ior`, and Cornell probes
carry no normal map so the distortion math is structurally a no-op even at
maximum strength).

**Local fog volumes (the other big Session 62 feature) have a matching
Cornell-coverage gap**: no `FogVolume` entity exists in the harness, and the
harness's *global* fog medium is fit to Bethesda-cell-scale numbers that
round to ~0 optical depth across a 14-unit box — so `--cornell` currently
returns a false all-clear for any fog regression, the same trap `#1942`
fixed for the sun path (REN-D21-01, REN-D21-02).

**Documentation drift, same shape as the fixed REN-DOC-2026-07-28-01
cluster.** `docs/engine/shader-pipeline.md` doesn't yet know about
`GpuFogVolume`, `MATERIAL_KIND_FIRE_REFRACTION` (103), or the six new
volumetrics descriptor bindings (Dim 3, Dim 16); `docs/engine/memory-budget.md`
describes a fixed 160×90×128 froxel grid that is now resolution-scaled and
understates peak VRAM ~2× (Dim 5); several in-shader comments cite stale line
numbers or stale rationales (Dim 13, Dim 19).

## RT Pipeline Assessment (Dimensions 1, 2, 9)

BLAS/TLAS build, refit, and deferred-destroy machinery is unchanged and
verified solid — Session 62's renderer work never touches AS build code
paths directly. The SSBO/`instance_custom_index` contract (the CRITICAL-floor
item) is intact end to end, confirmed independently in Dimensions 1 and 2.
GPU skinning + BLAS refit is untouched this session and all VUID-relevant
guards (#1790, #1145, bone-palette overflow) hold; the one new finding there
is a missing-test gap, not live corruption (REN-D9-01: no binary check that
the committed skin-shader `.spv` matches the constants it's supposed to bake
in — currently benign, but the mechanism that would catch drift doesn't
exist).

The two live defects in this tier are both about a *new* material kind's
interaction with the RT mask system, not the AS machinery itself:
`MATERIAL_KIND_FIRE_REFRACTION` staying in `SHADOW_MASK_OPAQUE` (REN-D2-01,
HIGH) and `MultiLayerParallax` refractors being a caustic *source* per the CPU
gate while never entering `SHADOW_MASK_GLASS` per the TLAS mask assignment
(REN-D14-01, MEDIUM — MLP refractors can receive their own caustic on their
own back face).

## GPU-Struct & Memory Assessment (Dimensions 3, 5)

`GpuInstance` (112 B), `GpuCamera` (336 B), and `GpuMaterial` (348 B) are all
confirmed unchanged and in full Rust↔GLSL lockstep this session — Session
62's one touch to `bindings.glsl` (`9ade7506`) was comment-only, verified by
reading the diff directly rather than trusting the commit message. The new
`GpuFogVolume` struct (64 B) has a size/align pin but no field-order lockstep
test — the exact gap class `feedback_shader_struct_sync.md` exists to close
(REN-D3-01), and its cluster-indexing constants are hand-written GLSL
literals instead of build-script-emitted (REN-D3-02), the same defect class
as two previously-fixed issues (#1190/#1401).

Memory-wise, every new Session 62 GPU resource (procedural density-noise
volumes, local-fog-volume SSBOs) is allocation-correct — created once,
tracked, torn down on all three exit paths, no leak found. The gaps are
documentation and one CPU-cost oversight: `memory-budget.md`'s volumetrics
section describes a grid that no longer exists (understates peak 4K VRAM
~2×, REN-D5-01), and the "boot-generated" density noise is in fact
regenerated on the CPU on every window resize (REN-D5-03, ~10⁷ hash
evaluations, a real if bounded stall).

## Denoiser/Composite & Volumetrics Assessment (Dimensions 8, 13, 16)

SVGF, TAA, and the composite reassembly are all in good shape and
Session 62's shader rework did not perturb their consumed contracts
(motion vectors, mesh-ID disocclusion, firefly clamps all re-verified
unchanged). The volumetric fog rework itself (Dimension 16, the largest
scope in this sweep) is structurally sound — config plumbing, dispatch
geometry, the hybrid-Z slice mapping (four independent implementations,
algebraically identical), and the authored-medium integration that closes
#2220 all check out. Three real shading defects remain inside the new code:
the camera-relative height-fog datum (HIGH, above), a rectangle-rule
slab-integration that lets dense local fog volumes over-brighten without
bound (REN-D16-03, MEDIUM), and a cluster/dedup fragility in
`sampleLocalMedium` whose only real protection against replaying a stale
cell's smoke is a single shader early-out (REN-D16-04, MEDIUM).

## Detailed Findings

### HIGH

#### REN-D15-01 — Water-side caustic splat refracts through the flat plane normal, not the wave normal
- **Dimension**: 15 (Water)
- **Location**: `crates/renderer/shaders/water.frag`, the `#1256` caustic block
- **Status**: NEW
- Caustics require refraction through *curved* (perturbed) geometry to focus
  light; the block uses `Nsurface` (constant `(0,1,0)` for every fragment of
  a flat water plane), not `Nperturbed`. The result is a rigid, structureless
  translation of the water plane's screen footprint onto the floor — visually
  indistinguishable from a lighting bug. The code's own header comment claims
  it refracts through "the bumped water normal," which it does not.
- **Fix**: swap `Nsurface` for `Nperturbed` in the `refract()` call and the
  Lambert weight; keep `Nsurface` only for the origin bias/side convention.

#### REN-D2-01 — Fire-refraction proxies remain `SHADOW_MASK_OPAQUE` occluders despite a comment claiming TLAS exclusion
- **Dimension**: 2 (SSBO/ray queries), corroborated by Dimension 1 (AS masks) and Dimension 11 (G-buffer overwrite)
- **Location**: `crates/renderer/shaders/triangle.frag` (fire-refraction branch); `crates/renderer/src/vulkan/acceleration/predicates.rs` (`shadow_mask_for_instance`)
- **Status**: NEW
- The shader comment says the proxy "is excluded from BLAS/TLAS, so this ray
  cannot hit the haze mesh itself and the proxy cannot cast shadows." The CPU
  side does no such exclusion — `shadow_mask_for_instance` hands fire-refraction
  proxies `SHADOW_MASK_OPAQUE`, so they occlude shadow rays from every other
  surface. A campfire's heat-haze plane produces a dark rectangle in the
  shadow term around the very light it's meant to be transparent to.
- **Fix**: either add the material kind to the shadow-transport skip
  predicate (matching `MATERIAL_KIND_EFFECT_SHADER`'s treatment), or actually
  exclude the proxy from the TLAS as the comment claims — pick one and add a
  positive test.

#### REN-D16-01 — Volumetric height-fog's reference altitude is the camera's own Y, not a world datum
- **Dimension**: 16 (Volumetrics)
- **Location**: `crates/renderer/shaders/volumetrics_inject.comp` (`proceduralDensityScale`); `composite.frag` (`heightFogOpticalDepth`)
- **Status**: NEW
- Both the froxel injector and the beyond-grid analytic continuation evaluate
  the exponential height profile against `params.camera_pos.y`. Fog density
  is therefore always maximal at eye level and follows the player vertically
  instead of thinning with real altitude — climbing a hill doesn't clear the
  fog. Worse, it breaks the new temporal-reprojection contract: vertical
  camera motion changes density at a fixed world point for reasons
  reprojection can't model, producing lag/ghost bands on stairs, elevators,
  and the fly camera.
- **Fix**: anchor the height-fog reference to a per-cell/world datum (ground
  height for exteriors, cell floor for interiors) instead of the camera.

#### REN-D18-01 — Session 62 made only the interior ambient cube authoritative — `is_exterior` and the whole TOD sky survive an exterior→interior transition
- **Dimension**: 18 (Sky/weather)
- **Location**: `byroredux/src/render/sky.rs` (`build_sky_params`)
- **Status**: NEW
- `3b922734` correctly made the interior XCLL cube override a stale exterior
  `SkyParamsRes`, per its own in-source comment stating that premise — but
  fixed only the `dalc_cube` field. Every other field, including
  `is_exterior: sky_res.is_exterior`, still flows unconditionally from the
  stale resource. Since #1199, `SkyParamsRes` is worldspace-scoped and
  survives cell unload/transition by design, and `SkyParamsRes` is *only ever*
  constructed with `is_exterior: true`. A sealed interior hides the symptom
  (the sky term is gated on `depth==1.0`), which is why the Session 62 author
  only saw the ambient-cube half of it; any interior with an unsealed
  roof gap or failed mesh gets full exterior TOD sky, sun disc, height fog,
  and the exterior volumetric shadow-ray path.
- **Fix**: make the interiority decision one read-side call — when
  `CellLightingRes.is_interior`, return `SkyParams { dalc_cube: interior_cube,
  ..SkyParams::default() }` rather than only overriding one field.

#### REN-D18-04 — #2218 (FO3 Megaton exterior whiteout) remains open, untouched by Session 62
- **Dimension**: 18 — status check only
- **Status**: Existing #2218
- Verified `3b922734`'s sky changes don't reach the FO3 exterior path (no
  DALC cube on FO3/FNV/Oblivion WTHR records). The HIGH-severity whiteout
  stands unmodified. Needs RenderDoc per the existing issue; the sky-side
  additive sun-glow terms are a plausible *contributor* but can't explain
  the reported white *geometry* since the sky branch is gated on `depth==1.0`.

### MEDIUM (selected — full list of ~33 across all dimensions in the per-dimension files)

- **REN-D1-01** — `SHADOW_MASK_OPAQUE` silently excludes glass; documented
  footgun for any future single-mask ray-query consumer, not a live bug today.
- **REN-D3-01/02** — new `GpuFogVolume` struct and its cluster constants lack
  the lockstep/build-script guards every other GPU struct has.
- **REN-D4-01** — the froxel temporal-history barrier's stated mechanism
  (intra-CB dependency) cannot actually cover the cross-frame write it
  claims to guard; the real safety net (both-slots fence wait) is real but
  undocumented at the barrier site.
- **REN-D5-01** — `memory-budget.md`'s volumetrics VRAM figures are stale by
  up to ~2× post-resolution-scaling.
- **REN-D6-01/02** — fire-refraction's third `ior` meaning is undocumented on
  the canonical type; 8 raw material-decision fields are still hand-translated
  at both NIF load sites outside the NIFAL boundary (a pre-existing gap, not
  new this session).
- **REN-D6-06** — #2203/#2204/#2209 (NIFAL collision defects) appear fixed by
  `3b922734` per `docs/engine/nifal.md`'s remediation note but remain open —
  likely a multi-issue-commit-close gap (only the first `Fix #N` reference
  auto-closes).
- **REN-D8-02** — bloom skipped on sky pixels (see Cross-Cutting Themes).
- **REN-D9-01** — no test verifies the committed skin-shader `.spv` matches
  the Rust-side stride/workgroup constants it must bake in.
- **REN-D10-01** — the new fog-volume system is an absolute-space GPU
  consumer with no `debug_assert` tying it to the documented RT precision
  ceiling.
- **REN-D11-02** — fire-refraction proxies overwrite the opaque receiver's
  G-buffer normal/motion at any coverage, including near-zero.
- **REN-D12-02** — fire-refraction's composition-phase sort key globally
  inverts back-to-front order against unrelated alpha-over transparents
  behind a proxy.
- **REN-D12-03** — status note on #2215 (see Executive Summary).
- **REN-D14-01/02** — MultiLayerParallax caustic-source/TLAS-mask mismatch;
  parked-camera EMA truncates dim caustics toward zero over time.
- **REN-D15-02** — authored WATR wave amplitude/frequency parsed and
  translated but never reach the GPU (pre-existing gap, documented in the
  2026-07-28 audit's prose, now tracked here explicitly).
- **REN-D16-02/03/04** — sky pixels get no volumetric term; rectangle-rule
  slab integration over-brightens dense local fog; cluster dedup fragility.
- **REN-D17-01/02** — `disneyDiffuseSplit` sheen weight disagrees by π
  between its two call sites; `pathEnvironmentRadiance` feeds DALC
  *irradiance* into the path integrator as if it were radiance (~π× too
  bright indirect floor on DALC-authored content).
- **REN-D19-01/02** — `perturbNormal`'s screen-space fallback double-flips
  handedness on mirrored UVs (affects terrain and all renderer-synthetic
  geometry); Starfield's packed bitangent sign isn't normalized to ±1 like
  every other game's, a latent primary/secondary-ray disagreement.
- **REN-D20-01** — a skipped frame can silently drop an egui texture delta,
  permanently darkening the debug overlay and leaking VRAM.
- **REN-D21-01/02/03** — Cornell-harness coverage gaps (see Cross-Cutting
  Themes).
- **REN-D22-01/02** — Session 62's new shadow-policy flag decode bypasses the
  existing per-game canonicalization boundary and reads raw TES5 bit layout
  unconditionally across all six games; the pre-existing animation-flag
  boundary itself silently assumes Skyrim's layout for FO76/Starfield too.

### LOW / Regression Guards

~55 items across all 23 dimensions — stale line-number citations (Dim 13),
stale doc comments (Dim 16, Dim 19, Dim 21), missing byte-parity tests on
shader-adjacent UBOs (Dim 14), and a long list of explicitly-reverified
invariants that still hold (AS build-flag constants, `instance_custom_index`
contract, TAA history lifecycle, FSR jitter singularity, egui render-pass
balance, GPU timer bracket/query alignment, Disney BSDF isotropic
degeneration, tangent-space Z-up/Y-up lockstep, and more). Full detail in
each dimension's `## What's Solid` and `## Findings` (guard-status) sections.

## Prioritized Fix Order

1. **Fix the two new-material-kind consistency gaps together**: fire-refraction
   TLAS/shadow-mask exclusion (REN-D2-01) and its G-buffer normal/motion
   overwrite (REN-D11-02) share one root cause — the proxy was designed as
   "excluded from everything but HDR color" but only partially wired that way.
2. **Fix water-side caustics' refraction normal** (REN-D15-01) — one-line
   shader change, high visual payoff, no sync/barrier risk.
3. **Fix the exterior→interior `SkyParamsRes` leak** (REN-D18-01) — single
   read-side change, closes a correctness gap that's been silently present
   since #1199 and just got worse in visibility this session.
4. **Anchor height-fog to a world datum instead of the camera** (REN-D16-01)
   — before the temporal-reprojection ghosting it causes gets attributed to
   something else.
5. **Apply bloom and volumetric fog to the sky branch** (REN-D8-02,
   REN-D16-02) — one composite.frag restructure fixes both.
6. **Close the process gap on #2203/#2204/#2209/#2215/#2220** — no code
   change, just issue hygiene, but prevents wasted re-investigation cycles.
7. **Add the missing GPU-struct lockstep test for `GpuFogVolume`**
   (REN-D3-01) before the next fog-related edit ships without it.
8. **Cornell-harness coverage**: add a `FogVolume` probe, a scaled-up fog
   medium, and a fire-refraction probe with `mat.set ior` support
   (REN-D21-01/02/03) — investment that pays off on every future renderer
   session, not just this one.
9. Everything else in the MEDIUM tier, roughly in dimension order, then LOW.

## Needs RenderDoc / Hardware Validation

- **#2218** FO3 Megaton exterior whiteout — unchanged, still needs a capture
  with isnan/isinf visualization.
- **REN-D11-02** fire-refraction G-buffer overwrite — needs a capture on a
  real fire cell to judge whether the normal/motion replacement is visually
  significant before touching the blend-attachment write masks.
- **REN-D23-07** FSR's `record_fsr_barriers_after` old-layout assumption —
  empirically validated (900-frame `BYRO_VALIDATION=1` sweep) against the
  current vendored SDK version; re-run that sweep as the gate on any future
  FidelityFX SDK version bump, not from static reasoning.
- Any barrier/layout change proposed anywhere above (there are none in this
  report beyond what's listed) should go through the same validation-layer
  gate per the project's standing anti-speculation policy.

## Verification

| Check | Result |
|---|---|
| `cargo test -p byroredux-renderer` | 503 passed, 0 failed (per Dimension 3/16's independent runs) |
| `cargo test -p byroredux --bins` | consistent with the 4186-passing workspace total from this session's `/session-close` |
| `scripts/check-shader-artifacts.sh` | PASS — all 21 shaders match pinned glslang 11:16.2.0 (verified independently by Dimensions 8, 11, 14) |
| Open renderer-labelled issue inventory | Refreshed; no duplicate filed — two issues (#2218, #2215, #2219, #2203/#2204/#2209) status-noted, not re-reported |
| Worktree before report | Clean |

No code fix and no GitHub issue publication were performed as part of this
audit. Suggested publication command:

`/audit-publish docs/audits/AUDIT_RENDERER_2026-08-02.md`
