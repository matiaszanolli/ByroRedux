# 2161: PERF-REGRESSION-6c56e311 (D5-01): Main-pass fragment shader ~2.2x slower since 2026-07-19 — needs a filed decision-tracking issue

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/2161
**Labels**: bug, high, performance

---

## Severity
HIGH

## Dimension
GPU Pipeline & Pass Efficiency (Dim 5) — `/audit-performance` 2026-07-25

## Status note
Tracked in ROADMAP.md Known Issues since 2026-07-24, but was **not yet filed as a GitHub issue** — verified against a fresh `gh issue list --state all --limit 1000` pull (zero matches for `6c56e311`, `regression-`, `traceshadow`, `path trac`, `fps`, `frame time`). Filing per the audit report's explicit recommendation, so this decision is tracked outside ROADMAP prose.

## Location
`crates/renderer/shaders/include/lighting.glsl:172-292` (`traceShadowTransmittance`); `crates/renderer/shaders/triangle.frag:2913-3022` (bounded GI path tracer); callers `triangle.frag:2657,2809`, `lighting.glsl:376,432`

## Description
Commit `6c56e311` ("Refactor volumetric lighting and water shaders", 2026-07-19) dropped Prospector from 149.6 FPS to 68.5 FPS. ROADMAP's own investigation (`git bisect` isolating `6c56e311`; a same-machine rebuild of the good parent ruling out environmental drift; a per-file SPIR-V swap isolating the cost to `triangle.frag` specifically, not the named volumetrics pass) is re-verified here line-by-line against current code at HEAD `ca7a4e0e` with no drift from the ROADMAP narrative (see ROADMAP.md:755-842).

## Evidence
`traceShadowTransmittance` (`lighting.glsl:172-292`) replaced a single any-hit `TerminateOnFirstHitEXT` probe with two sequential closest-hit walks — an 8-layer alpha-aware opaque walk (`MAX_OPAQUE_LAYERS = 8`, `lighting.glsl:179`) that unconditionally loads `GpuInstance` + `GpuMaterial` per hit before any early-out (`lighting.glsl:196-197`), plus a 4-interface glass walk (`MAX_GLASS_INTERFACES = 4`, `:233`) with per-interface Fresnel/Beer absorption. Worst case is 12 closest-hit queries per shadow ray, fired `SHADOW_RAYS = 4` times per light (`triangle.frag:2636,2657`) plus a pass-2 shadow-subtract (`:2809`). The GI ray became a bounded path tracer (`MAX_PATH_SEGMENTS = 6`, `MAX_DIFFUSE_BOUNCES = 2`, `triangle.frag:2913-2914`) where it was one `TerminateOnFirstHit` traversal; each diffuse-bounce hit re-invokes the same shadow-transmittance machinery, so the full GI path bounds at roughly 6 segments x 4 shadow calls x 12 queries ~= 288 nested ray queries per pixel on top of the direct path's 48. Both features were introduced by `6c56e311` itself.

## Impact
~2.2x frame time on real glass-heavy interior content. Now partially masked by FSR 3.1 Quality (the shipped default) shading fewer pixels — a symptom reduction, not a fix; ROADMAP is explicit this should not be read as lowering urgency. Also amplifies the cost side of D2-02 (opaque RT overdraw with no depth pre-pass, tracked as prior-audit-existing, not re-filed) and #779 (missing `early_fragment_tests`) — every occluded fragment that still runs the full shader before the depth test now pays this ~2.2x higher per-fragment cost. See also the PERF-D9-NEW-01 issue (same commit `6c56e311` also introduced a `camera_cut` false-positive) — the measured 68.5 FPS may already include some frames shading under a forced-reset (single-frame, no-motion-vector) state rather than steady-state temporal accumulation; the two regressions were never isolated from each other in the ROADMAP bisect, which only swapped the fragment shader binary.

Independently corroborated at the runtime-telemetry level by the concurrent `/audit-runtime` leg of this sweep: the fnv and skyrim_se `wall_fps` drops in `docs/audits/AUDIT_RUNTIME_2026-07-25.md` (fnv 147.3->138.7, skyrim_se 321.1->256.9) are plausibly this same regression (fps is advisory-only for that skill's gating, so it was not separately raised there).

## Related
ROADMAP.md:755-842; commits `6c56e311`, `e414249f` (good parent), `8a668eff` (bench-of-record), `ca7a4e0e` (shader-artifact parity CI check — this regression is a real source-level cost, not a build drift); the PERF-D9-NEW-01 issue (same originating commit, independent host-side bug).

## Suggested Fix
None proposed, deliberately — both features are intentional visual work (glass tints light instead of casting black shadows; second-bounce colour bleeding) and ROADMAP already measured and evaluated the available trade-off points (`ROADMAP.md:786-790`), including a rejected `SHADOW_MASK_SOLID` TLAS-bucket mitigation that measured +6% but introduced an unexplained 0.336%-of-pixels visual delta against a 0.000% noise floor — per the project's speculative-Vulkan rule, that needs RenderDoc evidence or a revert, not another attempt. The only open action is a product/quality decision (pick a point on the already-measured knob table, or accept the cost), not an engineering fix. Consider re-measuring after PERF-D9-NEW-01 is fixed, since the fixed baseline may show a different (likely smaller) regression magnitude.

## Completeness Checks
- [ ] N/A — this is a decision-tracking issue, not a code-fix issue
