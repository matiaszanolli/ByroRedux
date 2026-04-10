# #203: RL-11/12/13/14/15: Error-path resource leaks in Vulkan constructors
- **Severity**: MEDIUM — **Domain**: renderer — **Dimension**: Resource Lifecycle
- **Locations**: ssao.rs, compute.rs, scene_buffer.rs, texture_registry.rs, context/mod.rs
- **Fix**: Add error-path cleanup (scopeguard or closure pattern from build_blas)
