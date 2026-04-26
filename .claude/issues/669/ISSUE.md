# Issue #669: RT-4: GI ray tMin (0.5u) > bias (0.1u) — inverted size relationship vs reflection/shadow

**File**: `crates/renderer/shaders/triangle.frag:1497, 1503`
**Dimension**: RT Ray Queries

GI ray initialized with `giOrigin = fragWorldPos + N * 0.1` but `tMin = 0.5`. The bias (0.1u) is smaller than tMin (0.5u). On cosine-weighted hemisphere directions where `dot(giDir, N)` is small (sampled near the horizon), the first 0.5 units of the ray sweep through the surface plane before tMin starts accepting hits — large receivers can self-occlude themselves at grazing GI directions.

The reflection ray uses tMin = 0.01 (line 327); the glass refraction ray uses tMin = 0.05 (line 1063); the shadow ray uses tMin = 0.05 (line 1457). The 0.5-unit GI tMin appears anomalous against this pattern.

**Fix**: Either raise the bias to be commensurate with tMin (e.g. `N * 0.5`), or drop tMin to 0.05 like the shadow ray.

## Completeness Checks
- [ ] SIBLING: same pattern checked in related files
- [ ] DROP: if Vulkan objects change, verify Drop impl still correct
- [ ] LOCK_ORDER: if RwLock scope changes, verify TypeId ordering
- [ ] FFI: if cxx bridge touched, verify pointer lifetimes
- [ ] TESTS: regression test added for this specific fix

---
*From [AUDIT_RENDERER_2026-04-25.md](docs/audits/AUDIT_RENDERER_2026-04-25.md) (commit 20b8ef0)*
