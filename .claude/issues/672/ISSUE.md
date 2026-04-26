# Issue #672: RT-9: shadow ray jitter target uses radius * 0.025 with floor 1.5u — radius=0 lights collapse to point lights

**File**: `crates/renderer/shaders/triangle.frag:1425`
**Dimension**: RT Ray Queries

`lightDiskRadius = max(radius * 0.025, 1.5)` — the floor rescues `radius = 0` lights, but a 1.5-unit (~2 cm at Bethesda scale) disk produces ~zero penumbra at any room-scale shadow distance.

Real Bethesda XCLL light radii are 256–4096 units, so the `radius * 0.025 = 6.4..102 units` branch dominates in practice and the floor is dead. If REFR-imported lights ever ship `radius = 0` (e.g. bare `<NiPointLight>` or LIGH records without a published radius — see #277), they will silently lose penumbra.

**Fix**: Confirm the importer always writes `radius > 0` for visible lights, or change the floor to scale with the cell extent.

## Completeness Checks
- [ ] SIBLING: same pattern checked in related files
- [ ] DROP: if Vulkan objects change, verify Drop impl still correct
- [ ] LOCK_ORDER: if RwLock scope changes, verify TypeId ordering
- [ ] FFI: if cxx bridge touched, verify pointer lifetimes
- [ ] TESTS: regression test added for this specific fix

---
*From [AUDIT_RENDERER_2026-04-25.md](docs/audits/AUDIT_RENDERER_2026-04-25.md) (commit 20b8ef0)*
