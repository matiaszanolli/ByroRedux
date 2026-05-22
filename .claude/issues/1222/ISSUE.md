**Severity**: LOW
**Dimension**: Transform Compatibility (forward-looking ergonomics)
**Source**: `docs/audits/AUDIT_LEGACY_COMPAT_2026-05-19.md` D3-NEW-03

`spawn_precombined_meshes` at `byroredux/src/cell_loader/precombined.rs:61-65` currently hardcodes:

```rust
let pos = Vec3::ZERO;
let rot = Quat::IDENTITY;
let scale = 1.0;
```

with the comment "precombined NIFs are baked in cell-local coords so they sit at the cell origin with no rotation / scale." That's correct for the **interior** caller in `load.rs` but assumes the caller is itself at the cell origin. The helper accepts no explicit cell-origin argument, so the exterior caller (when D3-NEW-02 lands) can't pass a non-zero offset without a signature change.

### Impact
Today benign (no exterior caller, no CSG geometry). When D3-NEW-01 / D3-NEW-02 are addressed the helper will need an explicit `cell_origin: Vec3` parameter OR a documented invariant that callers must pre-translate.

### Suggested Fix
Extend the signature to take `cell_origin: Vec3` and apply it to the spawn transform. Mirror the existing `spawn_placed_instances` composition shape.

### Completeness Checks
- [ ] **SIBLING**: Both interior + exterior callers updated in lockstep.
- [ ] **TESTS**: N/A until CSG reader produces geometry.
