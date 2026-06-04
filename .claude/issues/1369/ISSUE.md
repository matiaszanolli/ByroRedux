**Severity**: MEDIUM · **Dimension**: GPU Pipeline (PERF-D1-NEW-01)
**Location**: `crates/renderer/shaders/triangle.frag:2684` (NUM_RESERVOIRS=16), `:2690-2698` (arrays), `:2702-2995` (streaming pass), `:3004-3103` (shadow-ray pass)
**Status**: NEW

After the 8→16 reservoir bump: ~192 interleavedGradientNoise evals + 192 conditional array writes per fragment at cluster.count≈12 (double the prior), and `uint[16]+float[16]+vec3[16]` = **320 B/thread local storage** (up from 160 B) suppresses occupancy across the *entire* clustered-lighting branch (BSDF + GI included), not just the reservoir code. Shadow-ray count is correctly bounded ≤16/fragment — this is occupancy/ALU/divergence, not a ray blowup.

**Fix** (do NOT ship blind — RenderDoc/Nsight occupancy capture before/after, per the speculative-Vulkan-fix policy): (1) make NUM_RESERVOIRS a spec-constant clamped to `min(16, cluster.count)`; (2) **drop `resRadiance[16]`** (192 B, the biggest array) and recompute radiance in pass 2 (which already recomputes L) — highest-leverage occupancy fix; (3) hoist the loop-invariant per-s noise offsets out of the ci×s double loop.

## Completeness Checks
- [ ] **UNSAFE**: If fix involves unsafe, safety comment explains the invariant
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **DROP**: If Vulkan objects change, verify Drop impl still correct
- [ ] **LOCK_ORDER**: If RwLock scope changes, verify TypeId ordering
- [ ] **FFI**: If cxx bridge touched, verify pointer lifetimes
- [ ] **CANONICAL-BOUNDARY**: If fix touches translate_material / Material::resolve_pbr / emitter params, keep per-game logic at the NIFAL boundary
- [ ] **TESTS**: Regression test added for this specific fix

_Filed from `docs/audits/AUDIT_PERFORMANCE_2026-05-31.md` (/audit-performance, deep)._
