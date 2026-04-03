# Investigation: Issue #16 — Normal transform ignores non-uniform scale

## Current State
- `triangle.vert:22`: `fragNormal = mat3(pc.model) * inNormal`
- `triangle.frag:16`: `normalize(fragNormal)` — already normalizes
- Push constants: viewProj (64 bytes) + model (64 bytes) = 128 bytes (Vulkan minimum limit)
- ByroRedux uses uniform scale only (f32, not Vec3) — so mat3(model)*normal + normalize is correct TODAY
- But the code is technically wrong for non-uniform scale

## Fix Options
1. **Compute inverse-transpose in vertex shader** — `transpose(inverse(mat3(model)))` is cheap on GPU.
   Pros: zero CPU/pipeline changes. Cons: per-vertex matrix inverse (3x3, ~30 ops — negligible).
2. **CPU normal matrix + UBO** — requires UBO infrastructure, descriptor set changes. Overkill.
3. **Normalize only** — already done in frag, vertex shader just adds a normalize(). Only correct for uniform.

## Decision: Option 1
Compute `transpose(inverse(mat3(pc.model)))` in the vertex shader. This is:
- Correct for ALL scale types (uniform, non-uniform, shear)
- Zero Rust code changes (shader-only fix)
- The standard approach used by every modern engine
- Negligible GPU cost (3x3 inverse is ~30 ALU ops per vertex)

## Files
1. `crates/renderer/shaders/triangle.vert` — fix normal transform
2. Recompile SPIR-V

**2 files — within threshold.**
