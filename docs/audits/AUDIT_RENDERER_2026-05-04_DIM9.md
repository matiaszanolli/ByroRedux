# Renderer Audit — 2026-05-04 (Dimension 9 only)

**Scope**: Dimension 9 — RT ray queries inside `triangle.frag` (every `rayQueryEXT` call site).
**Other dimensions**: not run (`/audit-renderer --focus 9`).
**Depth**: deep.

## Executive Summary

The RT ray-query path is in **strong shape** relative to the prior-audit
baseline. 31 ray-query API calls across 5 `rayQueryInitializeEXT` sites
(window portal, glass IOR refraction, traceReflection helper, metal
reflection, reservoir shadow loop, GI hemisphere) all pass the depth=deep
checklist with two surgical NEW findings: one MEDIUM (basis-NaN window on
glass-IOR roughness spread at normal incidence) and one LOW (window-portal
ray's intentional asymmetric origin bias is undocumented).

The headline carry-over from `AUDIT_RENDERER_2026-05-03_EXTERIOR.md` is
**#671 / RT-8** — GI miss path uses hardcoded `vec3(0.6, 0.75, 1.0) * 0.06`
at `triangle.frag:2127`. Re-confirmed open; not refiled here.

**Backlog cleanup observed**: three issues listed as "still open" in the
2026-05-03 audit's backlog table (**#733, #741, #742**) are **all closed
in current source AND closed in the issue tracker**. The 2026-05-03 audit
mechanically copied the row from the 2026-04-27 list without re-verifying.
No issue-tracker action needed; just remove the rows on the next audit.

| Severity | Count |
|----------|-------|
| CRITICAL | 0 |
| HIGH     | 0 |
| MEDIUM   | 1 |
| LOW      | 1 |

## Dimension 9 — RT Ray Queries

### Checklist results table

| # | Item | Result |
|---|------|--------|
| 1.1 | Shadow ray origin = `fragWorldPos + N_bias·0.05` (V-aligned) | PASS (line 1999) |
| 1.2 | Shadow direction toward light, jittered point/spot disk vs directional cone | PASS (lines 2003-2017) |
| 1.3 | Shadow tMin (0.05) ≥ bias (0.05) | PASS (line 2037) — RT-12 closed |
| 1.4 | Shadow tMax point/spot vs directional 100k | PASS (lines 2008, 2027) |
| 1.5 | Point/spot disk basis: Frisvad orthonormal around L | PASS — RT-2 / #574 fix |
| 1.6 | Directional jitter: `sunAngularRadius = 0.0047` rad (~physical sun) | PASS (line 2015) |
| 1.7 | `gl_RayFlagsTerminateOnFirstHitEXT` on shadow rays | PASS (line 2034) |
| 1.8 | Shadow contact-hardening / penumbra | DEFERRED (PCSS milestone) |
| 2.1 | Reflection ray origin biased along V-aligned `N_bias` | PASS (RT-3 / #668 closed) |
| 2.2 | Reflection direction = `reflect(-V, N_view)` with V-aligned flip | PASS |
| 2.3 | Metalness/roughness gating (>0.3 metal, <0.6 roughness) | PASS (line 1748) |
| 2.4 | Reflection-hit UV: 3-vertex barycentric, SSBO offsets 9..10 | PASS (lines 339-363) |
| 2.5 | Reflection-hit texture sample via `nonuniformEXT(hitTexIdx)` | PASS (line 406) |
| 2.6 | `gl_RayFlagsTerminateOnFirstHitEXT` on reflection | PASS (line 376) — #420 fix |
| 2.7 | BGSM UV transform applied to refl-hit UV | PASS (lines 402-403) — #494 |
| 3.1 | GI ray origin biased along `N_bias` (V-aligned) | PASS (line 2077) |
| 3.2 | GI direction = `cosineWeightedHemisphere(N)` with Frisvad | PASS (line 2076) |
| 3.3 | GI tMax = 6000 matches `smoothstep(4000, 6000)` fade end | PASS — RT-14 closed |
| 3.4 | GI tMin (0.05) consistent with bias (0.1) | PASS — #669 closed |
| 3.5 | GI miss returns sky color (no NaN) | code path PASS; **OPEN content** — #671 hardcoded sky |
| 3.6 | 1-bounce only constraint | PASS |
| 3.7 | IGN seed from `cameraPos.w` frame counter | PASS (lines 2071-2072) |
| 4.1 | Window portal direction = `-N` (not `-V`) | PASS (line 1318) — #421 closed |
| 4.2 | Window portal tMax = 2000 | PASS (line 1327) |
| 4.3 | Window portal grazing-angle gate (`windowFacing > 0.1`) | PASS (line 1317) |
| 4.4 | Window portal origin V-bias consistency with other sites | **PARTIAL** — REN-D9-NEW-02 |
| 5.1 | TLAS at `set=1, binding=2` `accelerationStructureEXT` | PASS — shader (line 160) ↔ Rust (`scene_buffer.rs:497`) |
| 6.1 | Every ray query gated by `rtEnabled = sceneFlags.x > 0.5` | PASS — all 5 sites verified |
| 7.1 | `rayQueryProceedEXT` paired correctly with terminate-on-first | PASS |
| 8.1 | Self-intersection avoidance: tMin > 0 AND V-aligned N bias | PASS (hoisted at line 966) |
| 8.2 | Bias accounts for surface curvature | NOTE — constant 0.05/0.1; held empirically across test cells |

### Findings

#### REN-D9-NEW-01: IOR-refraction roughness-spread basis NaN at normal incidence

**Severity**: MEDIUM
**Dimension**: RT Ray Queries × Shader Correctness
**Location**: [crates/renderer/shaders/triangle.frag:1457-1464](crates/renderer/shaders/triangle.frag#L1457-L1464)
**Status**: NEW

The Phase-3 glass IOR refraction adds a roughness-driven jitter inside the
`glassIORAllowed` block:

```glsl
float spread = roughness * 0.15;
if (spread > 0.001 && dot(refractDir, refractDir) > 0.0001) {
    vec3 rRight = normalize(cross(refractDir, N_geom_view));
    vec3 rUp    = cross(refractDir, rRight);
    refractDir  = normalize(refractDir
        + (rRight * (rn1 * 2.0 - 1.0)
        +  rUp    * (rn2 * 2.0 - 1.0)) * spread);
}
```

At **normal incidence** (camera looking dead-on at glass, `V` aligned with
`N_geom_view`), `refractDir` is parallel to `-N_geom_view`. Therefore
`cross(refractDir, N_geom_view) = (0, 0, 0)`, and `normalize((0,0,0))`
produces NaN per GLSL spec.

The `dot(refractDir, refractDir) > 0.0001` guard at line 1458 only catches
**total-internal-reflection** zeroing (when `refract()` returns
`(0,0,0)`). The TIR check never fires for air→glass (`eta = 1/1.5`), so
every glass material with `roughness > 0.0067` reaches this code path.

Vulkan ray-query spec (`VUID-RuntimeSpirv-OpRayQueryInitializeKHR-04347`)
requires finite ray geometry. NaN direction → undefined behaviour;
hardware-empirical observation ranges from "miss" to "validation error
in debug builds" to single-frame stochastic flicker on near-perpendicular
glass fragments at low non-zero roughness.

**Fix**: replace the manual basis with `buildOrthoBasis` (Frisvad, already
in use at GI / shadow / metal-reflection sites — same pattern that closed
RT-2 / #574):

```glsl
if (spread > 0.001 && dot(refractDir, refractDir) > 0.0001) {
    vec3 rRight, rUp;
    buildOrthoBasis(refractDir, rRight, rUp);
    refractDir = normalize(refractDir
        + (rRight * (rn1 * 2.0 - 1.0)
        +  rUp    * (rn2 * 2.0 - 1.0)) * spread);
}
```

One-line refactor; identical semantics as elsewhere; eliminates the
basis-NaN window.

#### REN-D9-NEW-02: Window-portal escape ray skips the V-aligned `N_bias` hoist

**Severity**: LOW
**Dimension**: RT Ray Queries × Shader Correctness (consistency)
**Location**: [crates/renderer/shaders/triangle.frag:1318-1328](crates/renderer/shaders/triangle.frag#L1318-L1328)
**Status**: NEW

Window-portal escape ray fires from `fragWorldPos - N * 0.15` along `-N`,
with **raw `N`** (the normal-mapped per-fragment normal). Every other RT
site in this shader hoists a single `N_bias = dot(N, V) < 0 ? -N : N` once
at line 966 and uses that.

The intent is correct (start *outside* the pane), and the
`windowFacing > 0.1` gate at line 1317 means surviving fragments always
have `dot(-V, N) > 0.1`, so `-N` always points away from the camera at
this code location. So applying the V-aligned `N_bias` here would actually
*invert* the bias direction and break the portal start position.

Pure documentation-level finding. No live correctness gap unless a future
refactor copy-pastes the `N_bias` pattern across without re-reading the
gate.

**Fix**: comment block at line 1318 explaining the intentional asymmetry,
or define a sibling `N_outward = -N_bias` once and use it. No semantic
change.

### Existing-issue cross-references (skip-don't-refile)

**Open and confirmed against current source**:

- **#671 / RT-8** — GI miss hardcoded `vec3(0.6, 0.75, 1.0) * 0.06` at
  `triangle.frag:2127`. Already filed; re-classified HIGH in
  `AUDIT_RENDERER_2026-05-03_EXTERIOR.md`. Strongest carry-over on this
  dimension.
- **RT-13** — No contact-hardening penumbra. Tracked as future PCSS milestone.

**Closed and verified in current source**:

- **#421** — Window portal `-V` → `-N` (`triangle.frag:1318`).
- **#420** — `gl_RayFlagsTerminateOnFirstHitEXT` on reflection (line 376).
- **#494** — BGSM UV transform on reflection-hit UV.
- **#574 / RT-2** — `buildOrthoBasis` Frisvad (no NaN window).
- **#668 / RT-3** — Reflection N_view flip on both sites.
- **#669 / RT-4** — GI tMin matches bias.
- **#789** — Glass IOR passthru loop with texture-equality skip + 2-passthru budget.

**Already closed in tracker — backlog table cleanup**:

`AUDIT_RENDERER_2026-05-03.md`'s "Prior-audit backlog" table lists three
items as still open. All three are confirmed CLOSED in the issue tracker
AND in current source — the 2026-05-03 audit copied the row from the
2026-04-27 list mechanically. No issue-tracker action needed.

- **#733 / RT-11** — Reservoir shadow N_bias V-flip. CLOSED, verified at
  `triangle.frag:1999` + hoist comment at lines 950-967 citing #733.
- **#741 / RT-12** — Reservoir shadow tMin=0.001→0.05. CLOSED, verified at
  line 2037.
- **#742 / RT-14** — GI ray tMax=3000→6000. CLOSED, verified at line 2091.

### Verified correct (no finding)

- **TLAS binding alignment**: shader (line 160) ↔ Rust (`scene_buffer.rs:497`); binding omitted entirely when `rt_enabled=false`.
- **`rtEnabled` gate**: every one of the 5 ray-query sites correctly gated on `sceneFlags.x > 0.5`.
- **`gl_RayFlagsTerminateOnFirstHitEXT`**: applied to every shadow / reflection / GI / window-portal / glass-IOR-passthru ray.
- **`rayQueryProceedEXT` semantics**: every Initialize followed by exactly one Proceed, no nested loops, no closest-hit traversal mixed with terminate-on-first flag.
- **Per-instance custom index**: every hit reads `rayQueryGetIntersectionInstanceCustomIndexEXT` (not `InstanceId`).
- **Frisvad orthonormal basis**: used for shadow disk (1996), GI hemisphere (332), metal-reflection cone (1765). The IOR-refraction site is the lone exception — REN-D9-NEW-01.
- **Vertex SSBO read safety**: `getHitUV` reads only float-safe offsets 9..10; `triangle_frag_no_unsafe_vertex_data_reads` static test rejects future drift.
- **Shadow disk basis** for point/spot: Frisvad around `L` (light direction), not screen-space — physically correct.
- **GI cosine-weighted hemisphere**: `r=sqrt(u1)`, `cos/sin(theta)`, `sqrt(1-u1)` vertical — correct.
- **Reservoir shadow subtraction**: `Lo = max(Lo - resRadiance[s] * W * shadowFade, 0)` clamped to zero.
- **Glass passthru loop**: 2-passthru budget bounded; texture-equality skip handles "ray hits own glass shell"; no infinite-loop possible.
- **Window-portal grazing-angle gate**: `windowFacing > 0.1` (~84° from normal).
- **Per-light shadow-type dispatch**: `lightType < 1.5` (point/spot disk) vs `lightType > 1.5` (directional cone).
- **Global N_bias hoist**: hoisted once at line 966 with explicit comment block citing #668 / RT-11 / #733.

## Suggested next step

```
/audit-publish docs/audits/AUDIT_RENDERER_2026-05-04_DIM9.md
```

And, on the next audit cycle, drop the **#733 / #741 / #742** rows from
the `AUDIT_RENDERER_2026-05-03.md` "Prior-audit backlog" table — those
fixes shipped.
