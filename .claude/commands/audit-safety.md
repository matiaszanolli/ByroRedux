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

### 3. Memory Safety
- GPU memory: all allocations freed before allocator drop
- GPU memory: allocator dropped before device destroy
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

## Process

1. `grep -r "unsafe" crates/ --include="*.rs"` to find all unsafe blocks
2. Read each unsafe block and its surrounding context
3. Check Vulkan resource pairing with Drop implementations
4. Save report to `docs/audits/AUDIT_SAFETY_<TODAY>.md`
