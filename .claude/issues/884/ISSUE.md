# Issue #884 (OPEN): CELL-PERF-08: expand_pkin_placements / expand_scol_placements allocate fresh Vec for common 1-element fanout

URL: https://github.com/matiaszanolli/ByroRedux/issues/884

---

## Description

`expand_pkin_placements_with_depth` (`byroredux/src/cell_loader_refr.rs:269-303`) allocates `Vec::with_capacity(pkin.contents.len())` even when the PKIN has 1 content. Same shape in `expand_scol_placements`. The "1-content PKIN" common case allocates a 1-element Vec on the heap when 99% of PKIN/SCOL fanout is ≤ 4 entries.

## Evidence

```rust
// cell_loader_refr.rs:281-302
fn expand_pkin_placements_with_depth(
    base_form_id: u32,
    outer_pos: Vec3,
    outer_rot: Quat,
    outer_scale: f32,
    index: &esm::cell::EsmCellIndex,
    depth: u32,
) -> Option<Vec<(u32, Vec3, Quat, f32)>> {
    let pkin = index.packins.get(&base_form_id)?;
    if pkin.contents.is_empty() { return None; }
    let mut out = Vec::with_capacity(pkin.contents.len());
    for &child_form_id in &pkin.contents {
        // ... recurse or push leaf
        out.push((child_form_id, outer_pos, outer_rot, outer_scale));
    }
    Some(out)
}
```

FO4 vanilla has 872 PKIN records; cell-scale PKIN counts are usually low (a few per cell), so this is a per-PKIN-REFR cost rather than a per-frame cost.

## Why it matters

Each PKIN/SCOL expansion that returns 1–4 entries pays a heap allocation it doesn't need. Stack-allocated `SmallVec<[_; 4]>` covers the common case; falls back to heap on overflow.

## Proposed Fix

Replace the return type with `smallvec::SmallVec<[(u32, Vec3, Quat, f32); 4]>`. The downstream consumer iterates the result the same way regardless of inline-vs-heap storage.

The same change applies to `expand_scol_placements` (per the audit; verify the call site).

## Cost Estimate

Small. Per-PKIN, not per-REFR. FO4 vanilla cells have a few PKINs each → handful of avoided heap allocs per cell load. Below the wall-clock signal floor today; rolled in for completeness.

## Completeness Checks

- [ ] **UNSAFE**: N/A (smallvec is safe).
- [ ] **SIBLING**: Audit other `Vec::with_capacity(small_count)` patterns in `cell_loader_refr.rs` / `cell_loader_scol_expansion.rs` / `cell_loader_pkin_expansion.rs`
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Existing PKIN/SCOL expansion tests must produce bit-identical results
- [ ] **DEPS**: Add `smallvec` to workspace deps if not already present (`cargo tree -p byroredux | grep smallvec` first)

## dhat Gap

Small expected savings. Per audit-performance command spec: warrants follow-up "wire dhat for expansion path" issue. dhat would catch any regression that re-introduces the heap-Vec pattern.

## References

- Audit: `docs/audits/AUDIT_PERFORMANCE_2026-05-06b.md` (CELL-PERF-08)
