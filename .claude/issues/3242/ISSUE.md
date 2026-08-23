# 3242: Incremental: MSWP per-shape swap loop breaks later-wins for duplicate-source entries

**Severity**: MEDIUM · **Report**: `docs/audits/AUDIT_INCREMENTAL_2026-08-23.md` (F1) · **Changed in**: `byroredux/src/cell_loader/spawn/mesh_instance.rs` (commit `900aa081`, Fix #973)

## Description

The new per-shape MSWP swap loop introduced by `900aa081` (#973):

```rust
let mut swapped = current.clone();
for entry in &refr_ov.material_swaps {
    if entry.source.eq_ignore_ascii_case(&swapped) && !entry.target.is_empty() {
        swapped = entry.target.clone();
    }
}
```

compares each entry's `source` against `swapped` — a variable that is reassigned on every match — instead of against the fixed original value. This breaks the documented and intended "later entry overrides" semantics for the MSWP format (per `refr.rs`'s own comment at line 379-380: "the spawn path applies them per shape with later-wins semantics matching the MSWP file format"):

- For two `material_swaps` entries with the **same** `source` (a duplicate BNAM→SNAM pair, legal in the MSWP format), only the **first** matching entry ever fires — the reverse of "later entry overrides."
- Conversely, if one entry's `target` happens to equal a *later* entry's `source` (an incidental string collision, not a duplicate), the two entries silently **chain** (A→B→C) — behavior nothing in the format or surrounding comments describes or intends.

This is a real, confirmed regression relative to the sibling reference implementation already in the same file family — `refr.rs`'s `build_refr_texture_overlay` (the original, single-`material_path`, REFR-level MSWP application from #971, unchanged in this diff) implements "later-wins" correctly by comparing every entry against the **fixed** `current` value:

```rust
// refr.rs:388-401 (unchanged, correct reference implementation)
for entry in &table.swaps {
    if entry.source.eq_ignore_ascii_case(&current) && !entry.target.is_empty() {
        ov.material_path = Some(pool.intern(&entry.target));
    }
}
```

## Impact

Narrow but real — triggers only when a single MSWP record lists more than one swap entry for the same source BGSM/BGEM (vanilla MSWPs average ~2.18 entries per the codebase's own count, so duplicates are plausible in denser variant tables, e.g. multi-tier color swaps touching the same base material twice). When it triggers, the wrong material variant is silently applied to that shape — visually wrong content, no crash, no error.

Note: issue #973's own suggested-fix pseudocode in its issue body carries the identical latent bug, so the shipped code matches what was specified — worth noting when fixing, since the spec itself needs correcting too.

## Suggested Fix

Compare `entry.source` against `current` (never mutated), not `swapped`, mirroring `refr.rs`'s existing correct loop — reassign `swapped = entry.target.clone()` only as the *output*, never as the comparison basis for the next entry.

## Completeness Checks
- [ ] **SIBLING**: Loop pattern matches `refr.rs:388-401` exactly (compare-against-fixed-value, assign-to-output)
- [ ] **TESTS**: A test with two `material_swaps` entries sharing the same `source`, asserting the *last* one wins (the exact gap the existing `mswp_swaps_apply_per_shape_not_just_the_overlay_material_path` / `mswp_filter_is_re_evaluated_per_shape` tests don't cover — each uses one swap per distinct source only)
