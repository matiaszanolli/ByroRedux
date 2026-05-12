# FO4-D4-NEW-08-followup: Apply MSWP material-swap list per-shape at mesh-spawn time

**Labels**: bug, renderer, low, legacy-compat

**Parent**: #971 (closed) — landed the parse-side XMSP arm + single-`material_path` substitution in `build_refr_texture_overlay`.
**Severity**: LOW (parent fix covered single-shape REFRs; this is the multi-shape case).
**Domain**: cell loader / mesh spawn / FO4 materials

## Premise

#971 landed three pieces of XMSP routing:

- `PlacedRef.material_swap_ref: Option<u32>` carries the XMSP FormID.
- `parse_refr_group` reads the XMSP sub-record.
- `build_refr_texture_overlay` resolves the MSWP, applies the FNAM path-prefix filter, and substitutes the overlay's single `material_path` when an entry's `source` matches. The resolved swap list is preserved on `RefrTextureOverlay.material_swaps` for downstream per-shape consumption — but **no consumer reads it yet**.

The per-mesh path-resolution loop is at [byroredux/src/cell_loader.rs:2263-2316](../../byroredux/src/cell_loader.rs#L2263-L2316). It iterates `imported.iter()` (one entry per NIF shape), and for each shape pulls a single `material_path` via `ov.and_then(|o| o.material_path).or(mesh.material_path)`.

## Gap

For a REFR that places a multi-shape mesh (e.g. a Raider armour with separate body / arm / leg shapes, each with its own authored BGSM), the overlay's single `material_path` only covers one shape — at most one shape's material gets the MSWP-substituted path. Every other shape resolves to its NIF-authored BGSM and the swap is silently ignored.

## Impact

The vanilla MSWP corpus averages ~2.18 swap entries per record (per `crates/plugin/src/esm/records/mswp.rs:25-28`). A typical use case (Raider armour `Variant04` swap) authors 3-4 (source → target) pairs that each target a different shape's BGSM. Today only one of those substitutions visibly fires per REFR. Visual symptom: a colour-variant Raider has the correct body-armour BGSM but unswapped arm/leg pieces.

## Suggested Fix

Inside the per-mesh resolve loop at `cell_loader.rs:2263-2316`, after the line that computes `material_path` (`cell_loader.rs:2286-2289`), apply MSWP per-shape:

```rust
let material_path = resolve_to_owned(
    &pool,
    ov.and_then(|o| o.material_path).or(mesh.material_path),
);
let material_path = if let (Some(ov), Some(current)) = (ov, material_path.as_deref()) {
    let filter_ok = |source: &str| {
        ov.material_swaps_filter
            .as_deref()
            .is_none_or(|f| source.to_ascii_lowercase().starts_with(&f.to_ascii_lowercase()))
    };
    let mut out = current.to_string();
    for entry in &ov.material_swaps {
        if entry.source.eq_ignore_ascii_case(&out) && filter_ok(&entry.source) && !entry.target.is_empty() {
            out = entry.target.clone();
        }
    }
    Some(out)
} else {
    material_path
};
```

Two notes:

1. The FNAM filter on `MaterialSwapRecord` is applied at overlay-build time today against the overlay's `material_path`. For per-shape application, the filter must be re-evaluated against **each shape's source path** — meaning the overlay needs to carry the `path_filter: Option<String>` alongside the swap list (add `material_swaps_filter: Option<String>` to `RefrTextureOverlay`, populated next to the existing `material_swaps`).
2. The `fill_from_bgsm` chain walk at `cell_loader_refr.rs:139-179` then needs to re-fire on the per-shape-swapped target — or the per-shape path resolution moves into a helper that walks the BGSM chain per shape. The current overlay does the chain walk once for the overlay's single `material_path`; that's no longer sufficient when per-shape applies different swap targets.

## Completeness Checks

- [ ] **PLUMBING**: `RefrTextureOverlay.material_swaps_filter: Option<String>` populated in `build_refr_texture_overlay` alongside `material_swaps`.
- [ ] **SIBLING**: Verify the single-`material_path` substitution already in `build_refr_texture_overlay` still works for single-shape REFRs (XATO MNAM-only path) — don't regress the parent fix.
- [ ] **SIBLING**: BGSM chain re-resolution per shape — either move `fill_from_bgsm` into the per-shape path or do an in-place provider lookup at the resolve site.
- [ ] **TESTS**: Regression test loads a synthetic multi-shape mesh with two authored BGSM paths, applies a 2-entry MSWP, asserts both shapes resolve to their swap targets.
- [ ] **TESTS**: Regression test verifies the FNAM filter is re-evaluated per shape (one shape matches the prefix, one doesn't — only the matching shape's path swaps).
- [ ] **OBSERVABILITY**: `mesh.info <entity_id>` debug command surfaces the per-shape material_path post-swap so the fix is visible without RenderDoc.

## Notes

Parent commit `4899b30` introduced `RefrTextureOverlay.material_swaps: Vec<MaterialSwapEntry>` — populated at overlay-build time, ready for this consumer. The remaining lift is roughly 20-30 lines + the FNAM-filter plumbing + tests.

Gate: not required to close until a vanilla multi-shape MSWP REFR is visually confirmed to need it (e.g. a Sanctuary cell load with `RaiderArmorBoss_Variant04` placements).
