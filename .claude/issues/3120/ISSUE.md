# REN-D3-01: committed triangle.frag.spv is stale — the shipped binary and its source disagree by one constant

**Issue**: #3120 — https://github.com/matiaszanolli/ByroRedux/issues/3120
**Labels**: `medium,renderer,pipeline,bug`
**Filed**: 2026-08-20 · comprehensive audit suite
**Report**: `docs/audits/AUDIT_RENDERER_2026-08-20.md`

---

**Severity**: MEDIUM
**Dimension**: GPU-Struct Layout / shader-binary lockstep
**Source**: `docs/audits/AUDIT_RENDERER_2026-08-20.md` (REN-D3-01)

## Location

- `crates/renderer/shaders/triangle.frag.spv` vs `crates/renderer/shaders/triangle.frag` (the `RENDER_DEBUG_MODE_MAX` guard in `main()`)
- `crates/renderer/src/shader_constants_data.rs` (`RENDER_DEBUG_VOLUMETRIC_TERM`, `RENDER_DEBUG_MODE_MAX`)

## Status

**NEW — a regression of an invariant a prior audit certified clean four days earlier.** `docs/audits/AUDIT_RENDERER_2026-08-16.md` §3 verified "all 21 are byte-identical". It broke inside this delta.

## Description

`2325c1de` (2026-08-17) added `RENDER_DEBUG_VOLUMETRIC_TERM = 8` and redefined `RENDER_DEBUG_MODE_MAX = RENDER_DEBUG_VOLUMETRIC_TERM`, which `build.rs` duly regenerated into `include/shader_constants.glsl`. `composite.frag` was recompiled and its `.spv` is current. **`triangle.frag.spv` was not** — it was last regenerated at `3d3e3a7b` (2026-08-16). The shipped binary therefore still encodes the *old* bound of 7.

## Evidence — how this was found (reproducible)

**All 21 GLSL sources were recompiled and byte-compared against their committed `.spv`.** Each was rebuilt with `glslangValidator -V -I. <shader>` (glslang 11:16.2.0) into a scratch directory and `cmp`-ed against the committed binary. **20 of 21 are byte-identical; `triangle.frag.spv` is not.**

```
$ cmp triangle.frag.spv /scratch/triangle.frag.spv
triangle.frag.spv /scratch/triangle.frag.spv differ: byte 98133, line 213
```

`spirv-dis` narrows the difference to **exactly one instruction operand** — the whole rest of the 331 308-byte module is identical:

```
committed:  %7626 = OpUGreaterThan %bool %7625 %uint_7
fresh:      %7626 = OpUGreaterThan %bool %7625 %uint_8
```

which is `if (!legacyDebugMode && debugMode > RENDER_DEBUG_MODE_MAX)` — the "unrecognised structured mode" contract-failure branch that writes magenta and returns.

Ground truth at HEAD:
```
crates/renderer/src/shader_constants_data.rs:491:pub const RENDER_DEBUG_VOLUMETRIC_TERM: u32 = 8;
crates/renderer/src/shader_constants_data.rs:492:pub const RENDER_DEBUG_MODE_MAX: u32 = RENDER_DEBUG_VOLUMETRIC_TERM;
```

## Impact

**Bounded today, but the invariant is broken.** `r.debug volumetric` (`RenderDebugMode::VolumetricTerm`, `render_debug.rs`) makes the shipped `triangle.frag` treat mode 8 as corrupt: every fragment takes the magenta early-out. All eight MRTs are still written before that return (locations 6/7 at the top of `main`, 1/2/3 before the guard, 0/4/5 inside it), and `composite.frag`'s mode-8 branch returns the mapped froxel field without reading `direct4`, so the displayed image is unaffected — the geometry pass is simply doing no work for that view.

The real cost is the broken invariant: **source and shipped binary disagree, nothing in `cargo test` notices, and the next person to recompile `triangle.frag` for an unrelated reason ships an unreviewed behaviour change bundled with theirs.**

Neither existing guard can see this failure mode:
- The stale-`.spv` guard (`reflect.rs`, #1447 — `every_committed_spv_*` block-size pins) is a **struct-size** check and is structurally blind to `#define` *value* drift.
- `shader_constants.rs`'s `correctness_debug_views_require_raw_frame_graph_output` loop over `1..=RENDER_DEBUG_MODE_MAX` reads GLSL **source**, not SPIR-V.

## Suggested Fix

**Recompile `triangle.frag.spv` with the documented plain `-V` invocation — NOT `-g0`.** The reflection test needs `OpName` to be present, and stripping it breaks that test. Per `feedback_triangle_frag_spv_recompile.md`, the resulting size increase under current glslang (roughly 132 KB → 154 KB on the historical incident) is **benign** — do not treat the larger binary as a sign the recompile went wrong.

```bash
cd crates/renderer/shaders
glslangValidator -V -I. triangle.frag -o triangle.frag.spv
```

Then **close the class**, because the existing guards provably cannot see it: either add a test that recompiles each GLSL source at test time and byte-compares against the committed `.spv`, or — if a build-time glslang dependency is unwanted — extend `build.rs` to fail the build when `shader_constants.glsl` is regenerated with different content while any `.spv` predates the change.

## Related

- #1447 (the last stale-`.spv` incident and the guard it produced)
- #3046 (REN-DOC-01, OPEN)
- `feedback_triangle_frag_spv_recompile.md`

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — re-run the recompile-and-compare sweep over all 21 shaders after the fix, not just `triangle.frag`
- [ ] **TESTS**: A regression test pins this specific fix (recompile-and-byte-compare, or a build.rs freshness gate — the block-size pin in `reflect.rs` is not sufficient)
