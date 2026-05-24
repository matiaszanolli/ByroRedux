# REN-D4-NEW-01: GBuffer Attachment Drop safety-net fires during panic-unwind from new()

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/1128
## Source Audit
`docs/audits/AUDIT_RENDERER_2026-05-16.md` — Dimension 4 (Render Pass & G-Buffer)

## Severity
**LOW** — debug-only stack pollution. Not a release-build issue.

## Location
- `crates/renderer/src/vulkan/gbuffer.rs:202-214` (Attachment::drop safety-net)
- `crates/renderer/src/vulkan/gbuffer.rs:248-281` (GBuffer::new error path)

## Status
**NEW** at HEAD `1608e6a2`

## Description
The `Attachment::drop` safety net at `gbuffer.rs:202-214` fires `debug_assert!(false)` if any image/view/allocation remains. The `new()` path at `:248-281` builds `gb` locally and on error calls `gb.destroy(...)` then returns. But if any of the 5 `allocate` calls panics (e.g. allocator lock poisoned), the `gb` local Drop runs — and it calls each attachment's Drop, each of which fires the safety-net assert. The pre-fix release path would log without panicking; the debug path now compounds the original panic with 5 nested `debug_assert!(false)` panics.

## Impact
Debug-only stack pollution on allocator-poison errors. Not a release-build issue (debug_assert is compiled out).

## Suggested Fix
Wrap the `debug_assert!(false)` in a `if !std::thread::panicking() { ... }` guard so the safety-net doesn't fire during unwind. Mirror the `GpuBuffer::Drop` pattern (#656) which already has this guard.

```rust
fn drop(&mut self) {
    if self.image != vk::Image::null() && !std::thread::panicking() {
        debug_assert!(false, "Attachment::drop called with live handle ...");
    }
}
```

## Completeness Checks
- [ ] **UNSAFE**: N/A — Drop body has no unsafe surface
- [ ] **SIBLING**: Search for other `debug_assert!(false)` in Drop impls: `grep -rn "debug_assert!(false)" crates/renderer/src/ | grep -B5 "impl.*Drop"`
- [ ] **DROP**: This issue IS the Drop check
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: A poison-injection test would exercise this, but the panic-unwind path is hard to test in cargo; rely on the sibling pattern from #656

## Related
- #656 (closed, established the `!thread::panicking()` guard pattern for `GpuBuffer::Drop`)