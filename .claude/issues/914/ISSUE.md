---
issue: 0
title: REN-D8-NEW-04: Add debug_assert on last_blas_addresses.len() == instance_count after swap
labels: renderer, medium, vulkan, safety
---

**Severity**: MEDIUM (defence-in-depth; future regression catcher)
**Source audit**: docs/audits/AUDIT_RENDERER_2026-05-09.md (Dim 8)

## Location

- `crates/renderer/src/vulkan/acceleration.rs` — `decide_use_update` + TLAS swap site

## Why it's a bug

`decide_use_update` correctly length-checks `cached_addresses` vs `current_addresses`, but no `debug_assert_eq!` pins `last_blas_addresses.len() == instance_count` after the swap. A future "skip empty tail instances" optimization could silently desync them and trigger a `primitiveCount`-mismatch on the next UPDATE-mode build.

## Fix sketch

After the `mem::swap`, add:

```rust
debug_assert_eq!(
    self.last_blas_addresses.len(),
    instance_count as usize,
    "TLAS instance bookkeeping desync — UPDATE will fail next frame"
);
```

## Completeness Checks

- [ ] **SIBLING**: Same invariant for static TLAS rebuild path.
- [ ] **TESTS**: No new test required — assertion is the test.

🤖 Filed by /audit-publish from docs/audits/AUDIT_RENDERER_2026-05-09.md
