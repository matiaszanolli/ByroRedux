# Issue #671: RT-8: GI miss uses hardcoded sky color, ignores per-cell ambient

**File**: `crates/renderer/shaders/triangle.frag:1539`
**Dimension**: RT Ray Queries

GI ray miss path uses a fixed clear-day blue `(0.6, 0.75, 1.0) * 0.06`, ignoring the per-cell ambient `sceneFlags.yzw` that XCLL/LGTM populates. In a red-lit interior (cave, sunset, magic), the GI miss term injects unauthored blue light from outside.

The window portal (line 947) and glass-refraction sky fallback (line 1074) use the same hardcoded color, but those are semantically "looking at sky" — the GI miss represents "no nearby geometry within 3000u", which is more often interior void than sky.

**Fix**: Either gate by an exterior flag (TODO: `sceneFlags.w` is unused) or fall back to `sceneFlags.yzw * 0.5` so the ambient color authority stays consistent.

## Completeness Checks
- [ ] SIBLING: same pattern checked in related files
- [ ] DROP: if Vulkan objects change, verify Drop impl still correct
- [ ] LOCK_ORDER: if RwLock scope changes, verify TypeId ordering
- [ ] FFI: if cxx bridge touched, verify pointer lifetimes
- [ ] TESTS: regression test added for this specific fix

---
*From [AUDIT_RENDERER_2026-04-25.md](docs/audits/AUDIT_RENDERER_2026-04-25.md) (commit 20b8ef0)*
