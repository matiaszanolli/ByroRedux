# Issue #679: AS-8-9: M29 build_skinned_blas — refit chain accumulates BVH inefficiency over time, no rebuild policy

**File**: `crates/renderer/src/vulkan/acceleration.rs:660-662`
**Dimension**: Acceleration Structures

Skinned BLAS use `PREFER_FAST_BUILD | ALLOW_UPDATE`. The M29 design comment (lines 657-660) explicitly chooses FAST_BUILD over FAST_TRACE because skinned refits happen every frame. Vulkan REFIT-only BLAS quality degrades over time as vertex motion exceeds the original BVH bounds — a long animation cycle (an NPC walking for 30s) eventually has the refit BLAS become noticeably slower to traverse than a fresh BUILD.

The renderer never periodically destroys + re-`build_skinned_blas` to reset the BVH. Result: traversal cost on long-lived skinned NPCs grows monotonically until the entity despawns or the cell unloads.

**Fix**: Track per-skinned-BLAS frame count or animated-bbox ratio; when it exceeds a threshold (e.g. every 600 frames, ~10s at 60 FPS, or when bbox grows > 2× the original), enqueue a fresh `build_skinned_blas` for that entity. Cheap because the slot already has its output buffer; only the AS build (one-time fenced cmdbuf) needs to re-run. Add telemetry on average refit count between rebuilds.

## Completeness Checks
- [ ] SIBLING: same pattern checked in related files
- [ ] DROP: if Vulkan objects change, verify Drop impl still correct
- [ ] LOCK_ORDER: if RwLock scope changes, verify TypeId ordering
- [ ] FFI: if cxx bridge touched, verify pointer lifetimes
- [ ] TESTS: regression test added for this specific fix

---
*From [AUDIT_RENDERER_2026-04-25.md](docs/audits/AUDIT_RENDERER_2026-04-25.md) (commit 20b8ef0)*
