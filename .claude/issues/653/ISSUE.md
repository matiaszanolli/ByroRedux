# Issue #653: DEN-3: SVGF / TAA history slot read is unsynchronized against producer-frame's write — masked today by per-frame fence

**File**: `crates/renderer/src/vulkan/svgf.rs:670-699`, `crates/renderer/src/vulkan/taa.rs:606-631`
**Dimension**: Denoiser & Composite / Sync

Frame `f` reads `history[(f+1)%2].view` as `prev_indirect_hist` / `prev_history`. The previous frame wrote that slot via compute SHADER_WRITE and emitted a post-dispatch SHADER_WRITE→SHADER_READ FRAGMENT_SHADER barrier (svgf.rs:732-741, taa.rs:648-668). That barrier targets only `out_ind_img` / `out_mom_img` / `out_img` for THIS frame's index.

When this frame becomes the consumer of that slot, the consumer is COMPUTE_SHADER (next SVGF / TAA dispatch), not FRAGMENT_SHADER. Strictly per Vulkan spec the producer's `dst_stage` must include every stage that will eventually read — FRAGMENT alone is insufficient if the next read is from COMPUTE.

Today the fence-based ordering between submissions makes this work in practice (the producer's queue submission has fully drained before the consumer's submission begins, because we WaitForFences on the per-frame fence). But this is **implicit serialization** — if MAX_FRAMES_IN_FLIGHT is raised to 3, or a future timeline-semaphore refactor relaxes the fence wait, the missing dst_stage in the producer barrier becomes a real hazard. Validation layers don't catch it because the submission-level fence implicitly orders everything.

**Fix**: In both `svgf.rs:735` and `taa.rs:664`, widen `dst_stage_mask` from `FRAGMENT_SHADER` to `FRAGMENT_SHADER | COMPUTE_SHADER`. Composite still reads from FRAGMENT, the next SVGF/TAA frame reads from COMPUTE — both must be covered.

## Completeness Checks
- [ ] SIBLING: same pattern checked in related files
- [ ] DROP: if Vulkan objects change, verify Drop impl still correct
- [ ] LOCK_ORDER: if RwLock scope changes, verify TypeId ordering
- [ ] FFI: if cxx bridge touched, verify pointer lifetimes
- [ ] TESTS: regression test added for this specific fix

---
*From [AUDIT_RENDERER_2026-04-25.md](docs/audits/AUDIT_RENDERER_2026-04-25.md) (commit 20b8ef0)*
