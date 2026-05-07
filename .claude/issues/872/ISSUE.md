# Issue #872 (OPEN): NIF-PERF-08: Arc::from(name) in walk.rs resolvers allocates instead of cloning existing Arc<str>

URL: https://github.com/matiaszanolli/ByroRedux/issues/872

---

## Description

`crates/nif/src/import/walk.rs:877` (`resolve_affected_node_names`) and `:909` (`resolve_block_ref_names`) both allocate fresh `Arc<str>` storage via `Arc::from(&str)` for every resolved name, even though the underlying field on `NiObjectNET.name` is already `Arc<str>`.

`HasObjectNET::name()` returns `Option<&str>` (a deref of the existing `Arc<str>`), so re-wrapping it via `Arc::from(&str)` always allocates a new heap buffer + copies the bytes. A refcount bump (`Arc::clone`) is the correct primitive.

## Evidence

```rust
// walk.rs:870-879 — resolve_affected_node_names
let Some(name) = net.name() else {
    continue;
};
if name.is_empty() {
    continue;
}
out.push(std::sync::Arc::from(name));   // ← fresh Arc<str> alloc per name
```

Identical pattern at `walk.rs:909` for `BSTreeNode` bone-list resolution.

## Why it matters

- Every `NiDynamicEffect.affected_nodes` resolution (point/spot/directional lights) and every `BSTreeNode.bone` resolution (FO4/FO76 SpeedTree) re-allocates string storage that could be a refcount bump.
- Per cell with many lights + tree LODs (e.g. exterior FNV cells), this multiplies. Cell-load critical path.

## Proposed Fix

Add `fn name_arc(&self) -> Option<&Arc<str>>` to `HasObjectNET`:
- Default implementation returning `None`
- Override on `NiObjectNET` storage (the actual `Arc<str>` owner)

Then both resolvers write:
```rust
if let Some(arc) = net.name_arc() {
    if !arc.is_empty() {
        out.push(Arc::clone(arc));
    }
}
```

Same refcount-promotion pattern as the original #248. Naturally pairs with #834 (NIF-PERF-07): both findings collapse to "Arc::clone replaces Arc::from(&str)" once the header storage is also promoted to `Vec<Arc<str>>`.

## Cost Estimate

Per-light, per-tree-bone-list resolution; cell-load tier. Quantitative measurement requires dhat infra (not yet wired).

## Completeness Checks

- [ ] **UNSAFE**: N/A (no unsafe involved)
- [ ] **SIBLING**: Audit `crates/nif/src/import/` for other `Arc::from(&str)` patterns where the source is an existing `Arc<str>`
- [ ] **DROP**: N/A (no Vulkan objects)
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Regression test: load a NIF with many `NiDynamicEffect.affected_nodes` references and verify that each unique name resolves to the same `Arc::strong_count > 1` rather than independent allocations

## dhat Gap

Expected savings are estimates; no quantitative regression guard exists today. This finding warrants a follow-up "wire dhat for the NIF parse + import loop" issue.

## References

- Audit: `docs/audits/AUDIT_PERFORMANCE_2026-05-06.md` (NIF-PERF-08)
- Related: #834 (NIF-PERF-07 — same pattern at NiUnknown sites; pairs with this)
- Related: #248 (original `NiUnknown.type_name: Arc<str>` promotion)
