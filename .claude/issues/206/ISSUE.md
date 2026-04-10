# #206: RL-6: StagingPool has no Drop impl
- **Severity**: LOW — **Domain**: renderer — **Dimension**: Resource Lifecycle
- **Location**: `crates/renderer/src/vulkan/buffer.rs:14-114`
- **Fix**: Add Drop impl with log::warn + debug_assert if free_list non-empty
