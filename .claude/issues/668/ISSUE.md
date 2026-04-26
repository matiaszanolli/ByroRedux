# Issue #668: RT-3: reflection ray bias + N * 0.1 lacks the dot(N,V) flip the glass path uses

**File**: `crates/renderer/shaders/triangle.frag:1216`
**Dimension**: RT Ray Queries

`traceReflection(fragWorldPos + N * 0.1, jitteredR, 5000.0)` uses the unclamped vertex normal `N` (already perturbed by normal map at 638). When the bump map happens to push `N` such that `dot(N, V) < 0` (back-facing micro-surface — possible on grazing views or noisy normal maps), `+ N * 0.1` moves the origin BEHIND the macro surface into the reflector, and the ray either self-hits or punches through.

The glass-IOR path at line 1018 already handles this with `N_view = dot(N, V) < 0.0 ? -N : N` and biases by `+ N_view * 0.05`; the metal/glossy reflection path at 1216 lacks the V-aligned flip.

**Fix**: Reuse the same `N_view` pattern before the bias:
```glsl
vec3 N_view = dot(N, V) < 0.0 ? -N : N;
traceReflection(fragWorldPos + N_view * 0.1, jitteredR, 5000.0);
```

## Completeness Checks
- [ ] SIBLING: same pattern checked in related files
- [ ] DROP: if Vulkan objects change, verify Drop impl still correct
- [ ] LOCK_ORDER: if RwLock scope changes, verify TypeId ordering
- [ ] FFI: if cxx bridge touched, verify pointer lifetimes
- [ ] TESTS: regression test added for this specific fix

---
*From [AUDIT_RENDERER_2026-04-25.md](docs/audits/AUDIT_RENDERER_2026-04-25.md) (commit 20b8ef0)*
