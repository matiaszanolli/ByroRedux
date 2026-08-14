# REN-D2-04: traceReflection's hitBase is written and never read

- **Issue**: [#2919](https://github.com/matiaszanolli/ByroRedux/issues/2919)
- **Finding ID**: `REN-D2-04`
- **Labels**: `low,renderer,tech-debt,bug`
- **Source report**: [`docs/audits/AUDIT_RENDERER_2026-08-14.md`](../../../docs/audits/AUDIT_RENDERER_2026-08-14.md)
- **Run**: `/audit-suite rt-deep`, 2026-08-14, HEAD `205744ae`

> Immutable snapshot of the issue *as filed* (TD10-001 / #1156). GitHub is
> authoritative for current state — query `gh issue view 2919 --json state`.

---

- **Severity**: LOW
- **Dimension**: Ray Queries
- **Location**: `traceReflection` in `crates/renderer/shaders/include/raytrace.glsl`
- **Status**: NEW
- **Description**: `vec4 hitBase = vec4(0.0);` is declared alongside the other committed-hit
  carry-outs (`hitInstanceIdx`, `hitPrimitiveIdx`, `hitBary`, `hitUV`) and assigned
  `hitBase = candidateBase;` when the loop commits, but nothing after the loop reads it — the
  committed surface is re-sampled from scratch as `hitBaseRgb = sampleRayHitBase(hitInst,
  hitMat, hitUV, mipBias).rgb` because the coverage probe deliberately samples at LOD 0 while
  the shading sample needs the roughness mip bias. The dead local is a trap for the next
  reader, who may reasonably assume the coverage sample is being reused and "optimise away"
  the second fetch, silently dropping the roughness-scaled blur that makes rough-metal
  reflections noise-free.
- **Evidence**: `grep -n "hitBase" crates/renderer/shaders/include/raytrace.glsl` → line 72
  (declaration), 106 (assignment), and then only `hitBaseRgb` at 157/158/166. The three
  siblings that *are* read (`hitPrimitiveIdx`, `hitBary`, `hitUV`) sit in the same block.
- **Impact**: None at runtime — glslang dead-code-eliminates the local. Maintenance hazard
  only.
- **Related**: #1029 (the `traceReflection` return contract this block feeds), #1017.
- **Suggested Fix**: Delete `hitBase` and the `hitBase = candidateBase;` assignment, and note
  at the `sampleRayHitBase` call why the coverage probe's LOD-0 sample cannot be reused.

---

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, the sibling BLAS/TLAS path)
- [ ] **TESTS**: A regression test pins this specific fix

---

*Filed by `/audit-publish` from [`docs/audits/AUDIT_RENDERER_2026-08-14.md`](docs/audits/AUDIT_RENDERER_2026-08-14.md) — `/audit-suite rt-deep`, 2026-08-14, HEAD `205744ae`. Verified CONFIRMED against current code at publish time.*
