# AR-10: collect_text_key_events allocates Vec<String> per frame per entity

## Finding: AR-10 (LOW)

**Source**: `docs/audits/AUDIT_LEGACY_COMPAT_2026-04-15.md`
**Dimension**: Animation Readiness
**Games Affected**: All
**Location**: `crates/core/src/animation/text_events.rs:11`, `crates/core/src/animation/stack.rs:199-211`
**Related**: #229 (keyframe alloc — distinct issue)

## Description

Both `collect_text_key_events()` and `collect_stack_text_events()` return freshly allocated `Vec<String>` with cloned Strings every frame for every animated entity. For most frames, no text keys are crossed and the allocation is wasted (empty Vec). The String clones from `clip.text_keys` are heap allocations even when no event fires.

## Evidence

```rust
// text_events.rs:11
pub fn collect_text_key_events(...) -> Vec<String> {
    let mut events = Vec::new();  // allocated every call
    ...
}
```

## Impact

Per-frame allocation pressure proportional to number of animated entities. Minor for small scenes, measurable at scale (100+ animated NPCs).

## Suggested Fix

Return a small-vec or iterator instead of Vec. Use `Arc<str>` for text key labels to avoid clone overhead. Consider a callback/visitor pattern.

## Completeness Checks
- [ ] **UNSAFE**: If fix involves unsafe, safety comment explains the invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, verify Drop impl still correct
- [ ] **LOCK_ORDER**: If RwLock scope changes, verify TypeId ordering
- [ ] **FFI**: If cxx bridge touched, verify pointer lifetimes
- [ ] **TESTS**: Regression test added for this specific fix

_Filed from audit `docs/audits/AUDIT_LEGACY_COMPAT_2026-04-15.md`._
