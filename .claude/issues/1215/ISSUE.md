**Severity**: LOW (observability)
**Dimension**: NIF Format Readiness
**Source**: `docs/audits/AUDIT_LEGACY_COMPAT_2026-05-19.md` Dim 2 FIND-1

`parse_and_import_nif` at `byroredux/src/cell_loader/references.rs:812-890` returns `Some(Arc::new(CachedNifImport { meshes, ... }))` unconditionally when the underlying `parse_nif` succeeds — even when `import_nif_with_collision_and_resolver` produces an empty `meshes` Vec. The only diagnostics on the path are `log::warn!` for truncation and `log::debug!("Skipping editor marker NIF")` for BSX 0x20.

For the Diamond City Dugout Inn case (#1188), this means a `_oc.nif` whose every `BSTriShape` has `num_vertices=0` (Shared variant, CSG-deferred) returns an empty `CachedNifImport` and the operator gets **no log at all** that the file produced no geometry.

### Impact
The 2026-05-19 Dugout Inn debugging session required adding diagnostic logs to discover the zero-vertex condition. The `_oc.nif` Shared variant will keep producing zero-mesh imports until a CSG reader lands; until then operators need an out-of-the-box signal.

### Suggested Fix
At the end of `parse_and_import_nif`, when `meshes.is_empty() && collisions.is_empty() && lights.is_empty() && particle_emitters.is_empty() && embedded_clip.is_none()`, emit:

```rust
log::warn!(
    "NIF '{}' imported with zero meshes / collisions / lights / emitters / clips \
     — likely CSG-deferred (`_oc.nif` Shared variant, #1188) or pure marker scene",
    label,
);
```

The cell-loader can keep returning `Some(Arc::new(...))` so cache invariants don't change; only the observability gap closes.

### Completeness Checks
- [ ] **UNSAFE**: N/A.
- [ ] **SIBLING**: Same one-shot warn at `scene/nif_loader.rs` (loose NIF path).
- [ ] **TESTS**: log-capture test or eyeball-verify via Dugout Inn rerun.
