# Issue #663: CMD-4: UI overlay relies on inherited dynamic depth/cull state from last main batch

**File**: `crates/renderer/src/vulkan/context/draw.rs:1396-1436`
**Dimension**: Command Recording

UI overlay path defensively re-binds viewport + scissor (comment at 1403 acknowledges this) but does NOT touch `cmd_set_depth_test_enable`, `cmd_set_depth_write_enable`, `cmd_set_depth_compare_op`, `cmd_set_depth_bias`, `cmd_set_cull_mode`. The UI pipeline (`pipeline_ui`) inherits whatever the last batch in the main loop happened to set.

Today this works because the UI quad is rendered last and the inheritance accidentally aligns with UI needs (depth test off, write off, no cull). If the UI pipeline declares any of those as dynamic state and the last main batch is, say, a sky dome with z_write=0, the next frame's UI would render with stale depth-write state until a future opaque batch flipped it back.

**Fix**: Either (a) make the UI pipeline use static (non-dynamic) depth/cull state so the bind alone establishes it, OR (b) explicitly set every dynamic state the UI pipeline declares right after `cmd_bind_pipeline(pipeline_ui)`.

## Completeness Checks
- [ ] SIBLING: same pattern checked in related files
- [ ] DROP: if Vulkan objects change, verify Drop impl still correct
- [ ] LOCK_ORDER: if RwLock scope changes, verify TypeId ordering
- [ ] FFI: if cxx bridge touched, verify pointer lifetimes
- [ ] TESTS: regression test added for this specific fix

---
*From [AUDIT_RENDERER_2026-04-25.md](docs/audits/AUDIT_RENDERER_2026-04-25.md) (commit 20b8ef0)*
