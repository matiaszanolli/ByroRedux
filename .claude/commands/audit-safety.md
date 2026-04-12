---
description: "Safety audit — unsafe blocks, memory leaks, undefined behavior, Vulkan spec violations"
---

# Safety Audit

Read `_audit-common.md` and `_audit-severity.md` for shared protocol.

## Dimensions

### 1. Unsafe Rust Blocks
- List every `unsafe` block in the codebase
- For each: is there a safety comment? Is the invariant actually upheld?
- Common risks: dangling pointers, aliasing violations, uninitialized memory
- Focus on World::get() raw pointer extension and any FFI boundaries

### 2. Vulkan Spec Compliance
- All vkCreate*/vkDestroy* paired correctly
- No use-after-destroy (check Drop ordering)
- Validation layers enabled in debug — run and check for ANY errors
- Queue submission ordering correct (wait before signal)
- Acceleration structure builds: correct geometry flags, valid device addresses
- TLAS UPDATE mode: instance count and geometry count must match original BUILD
- Ray query extension: `VK_KHR_ray_query` enabled before use, feature gate checked

### 3. Memory Safety
- GPU memory: all allocations freed before allocator drop
- GPU memory: allocator dropped before device destroy
- GPU memory: BLAS scratch buffer, TLAS instance/result buffers, G-buffer images, SVGF history buffers all tracked and freed
- CPU memory: no unbounded growth (Vec without clear, HashMap without remove)
- Stack overflow risk: no deep recursion without bounds

### 4. Thread Safety
- RwLock: no potential for deadlock (TypeId ordering enforced?)
- Arc<Mutex<Allocator>>: lock held for minimum duration?
- Send + Sync bounds on Component and Resource traits correct?

### 5. FFI Safety (cxx bridge)
- C++ exceptions: does cxx handle them correctly?
- String lifetime: Rust strings passed to C++ — valid for duration of call?
- No raw pointer exchange across FFI without clear ownership

### 6. RT Pipeline Safety
- BLAS/TLAS device address queries: buffers must have SHADER_DEVICE_ADDRESS usage
- Global vertex/index SSBO: `instance_custom_index` bounds not checked on GPU — verify CPU-side encoding is correct
- Ray query origin bias: self-intersection avoidance (tMin > 0 or offset along normal)
- TLAS refit: `last_blas_addresses` comparison must handle mesh registry changes (add/remove)

## Process

1. Use Grep to find all `unsafe` blocks in `crates/` (`.rs` files)
2. Read each unsafe block and its surrounding context
3. Check Vulkan resource pairing with Drop implementations
4. Check RT-specific safety (acceleration structures, device addresses, SSBO indexing)
5. Save report to `docs/audits/AUDIT_SAFETY_<TODAY>.md`
