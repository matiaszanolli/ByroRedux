# Issue #670: RT-6: caustic ray tMin = 0.0 with no normal-aligned bias

**File**: `crates/renderer/shaders/caustic_splat.comp:230`
**Dimension**: RT Ray Queries

`G + refr * 0.5, 0.0, refr, 1000.0`. The origin offset rides the *refracted* direction (which by Snell's law bends through the surface), so for high-incidence rays the offset can stay close to the surface plane. tMin = 0.0 then accepts re-intersection with the refractor itself.

Conventional pattern (used in triangle.frag GI line 1497 and shadow line 1419) is `origin + N * bias` plus a non-zero tMin.

**Fix**: `G - N * 0.1, 0.05, refr, 1000.0` — mirror the triangle.frag refraction path (line 1063): origin steps into the glass along the normal, then tMin steps out.

(Bundle with SH-4 — same code site, same root issue.)

## Completeness Checks
- [ ] SIBLING: same pattern checked in related files
- [ ] DROP: if Vulkan objects change, verify Drop impl still correct
- [ ] LOCK_ORDER: if RwLock scope changes, verify TypeId ordering
- [ ] FFI: if cxx bridge touched, verify pointer lifetimes
- [ ] TESTS: regression test added for this specific fix

---
*From [AUDIT_RENDERER_2026-04-25.md](docs/audits/AUDIT_RENDERER_2026-04-25.md) (commit 20b8ef0)*
