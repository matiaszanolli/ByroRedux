# #751: SF-D3-03: `merge_bgsm_into_mesh` silently falls through on `.mat` paths — no warn, no log

URL: https://github.com/matiaszanolli/ByroRedux/issues/751
Labels: bug, import-pipeline, high, legacy-compat

---

**From**: `docs/audits/AUDIT_STARFIELD_2026-04-27.md` (Dim 3, SF-D3-03)
**Severity**: HIGH
**Status**: NEW

## Description

`byroredux/src/asset_provider.rs:435-535` (`merge_bgsm_into_mesh`) dispatches on `path.ends_with(".bgsm")` then `path.ends_with(".bgem")`, then `else { return false }`. A Starfield NIF whose stopcond captured `materials/foo.mat` lands in the else branch with no warn / log — the renderer pulls whatever the NIF stopcond stub left on the mesh: empty texture_path, empty normal_map, default PBR scalars, no two_sided, no alpha test.

## Evidence

`asset_provider.rs:435` opens with `if path.ends_with(".bgsm")`, `:504` with `else if path.ends_with(".bgem")`, and the function falls to `false` for any other path. There's no else-branch logging.

## Impact

Once SF-D3-01 lands (suffix-gated stopcond), Starfield NIFs that reference `.mat` paths in `Name` will populate `material_path` correctly — but `merge_bgsm_into_mesh` will silently return false on every one. Operators see zero-textured surfaces with no diagnostic in the log. Compounds with SF-D5-01 (BSGeometry import gap) to make Starfield mesh debugging especially painful.

## Suggested Fix

Two-stage:

**Stage A (cheap, this issue)**: Add a once-per-path warn log in the else branch so missing `.mat` parser is visible during cell loads:

```rust
} else {
    log_once_per_path!("starfield .mat material referenced but unsupported: {}", path);
    return false;
}
```

**Stage B (separate, larger)**: Wire a stub Starfield `.mat` JSON parser that at minimum extracts texture paths so the renderer has something to draw. Tracked separately (see SF-D6-03 planning).

## Completeness Checks

- [ ] **SIBLING**: Verify `MaterialProvider` chain at `asset_provider.rs:271-374` doesn't have its own silent-fail path for unknown extensions.
- [ ] **TESTS**: Test that a `.mat` path returns false AND emits the log line.
- [ ] **DROP / LOCK_ORDER / FFI**: n/a.

## Related

- SF-D3-01 (suffix-gated stopcond — feeds `.mat` paths into this function)
- SF-D6-03 (Starfield .mat parser — Stage B above)
