# #3640: FO4-2026-08-30-D4-02: APP_CULLED geometry with a live visibility controller is dropped at import — 13 files can never become visible

**Source**: `docs/audits/AUDIT_FO4_2026-08-30.md` — Dimension 4
**Severity**: LOW
**Location**: `crates/nif/src/import/walk/mod.rs` — the `shape.av.flags & 0x01` early-returns (four shape sites, plus the node siblings)

## Description

APP_CULLED (`flags & 0x01`) geometry is dropped at import. Some of that geometry ships with a
visibility controller whose entire purpose is to un-hide it at runtime — and because the
shape never reaches the ECS, no controller can ever make it visible.

## Evidence

Current code (verified 2026-08-30) — the drop is an unconditional early return, repeated at
four shape sites and four node sites:

```rust
if shape.av.flags & 0x01 != 0 {
    return;
}
```

MEASURED over the FO4 corpus: **581 of 136,948 non-`_oc` BSTriShapes (0.42%) are
APP_CULLED**. **14 files have all shapes culled** and import to zero geometry, and **13 of
those 14 also ship a `NiVisController` / `NiBoolTimelineInterpolator` /
`BSNiAlphaPropertyTestRefController`** targeting them.

(For context: this survived a broader "16.2% of non-`_oc` NIFs import to zero meshes"
candidate that was otherwise dropped — 6,818 of those are collision-only `_physics.nif`
siblings, 258 camera-only, and the remaining drops are correct. This 13-file residue is the
part that is not correct.)

## Impact

13 files whose geometry is authored to appear under controller control can never appear.
Small in count, but it is a permanent one-way loss at import: no downstream system can
recover a shape that was never created.

## Suggested Fix

Import culled shapes with `visible = false` when a visibility channel targets them, rather
than dropping them — the controller then has something to toggle. Keep the unconditional drop
for culled shapes with no visibility channel.

## Related

#165 / audit N26-4-06, #332 (the pre-#332 `0x21` mask conflation with `DISPLAY_OBJECT_MASK`,
already fixed and not part of this defect).

## Completeness Checks
- [ ] **SIBLING**: the same `flags & 0x01` early-return appears at eight sites in `walk/mod.rs` (four shape, four node) — a fix at one is not a fix
- [ ] **TESTS**: a regression test pins one of the 13 measured files importing its shapes with `visible = false` and a live controller target
