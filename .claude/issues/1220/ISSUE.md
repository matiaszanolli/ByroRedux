**Severity**: HIGH
**Dimension**: Transform Compatibility (cross-cuts with #1188 scope)
**Source**: `docs/audits/AUDIT_LEGACY_COMPAT_2026-05-19.md` D3-NEW-01

The exterior CELL walker in `crates/plugin/src/esm/cell/wrld.rs:354-361` initialises both new precombined fields to empty with a code comment that reads:

```rust
// Exterior cells don't author XCRI / XPRI
// (FO4 precombines are interior-only in
// vanilla; mod content rare). Empty here
// is safe — the cell loader skips the
// precombined-spawn step when both are
// empty. #1188.
precombined_mesh_hashes: Vec::new(),
absorbed_refs: std::collections::HashSet::new(),
```

This premise is **factually wrong**. FO4's PreCombined Mesh system was designed primarily for **exterior** cells — Commonwealth open-world tiles (Concord, Sanctuary Hills, Boston downtown, Diamond City Marketplace) ship per-tile precombined NIFs that bake the full architectural facade into a single asset; this is the FO4 performance headline feature documented as "Previs+PreCombined" in CK docs. `Fallout4 - MeshesExtra.ba2` ships 124,871 `_oc.nif` files — far more than the vanilla interior count.

### Evidence
- The interior walker in `crates/plugin/src/esm/cell/walkers.rs:158-204` correctly parses XCRI / XPRI sub-records on interior CELL records.
- The exterior walker in `crates/plugin/src/esm/cell/wrld.rs:198-371` iterates sub-records but never matches `b"XCRI"` or `b"XPRI"`. Hardcodes both to empty.
- File count: `Fallout4 - MeshesExtra.ba2` contains 124,871 `_oc.nif` entries (probed 2026-05-19).

### Impact
Today: silent under-coverage of the XPRI absorption set — no functional regression because precombined-spawn returns 0 anyway. Tomorrow (CSG-reader milestone): every Commonwealth exterior cell would skip the optimisation and pay the per-REFR draw cost.

### Related
- #1188 (today's commit, eeddc81b)
- `docs/audits/POST_MORTEM_2026-05-19_PRECOMBINED.md` should be updated to flag the exterior parse-side gap as the second leg of the same audit miss.

### Suggested Fix
Lift the XCRI / XPRI sub-record arms from `crates/plugin/src/esm/cell/walkers.rs:158-204` into `wrld.rs`'s sub-record loop and assign the parsed values to the construct fields. Mirror the interior path's `mesh_count + ref_count` header decode and `n × u32` tail. Add a regression test against a known-precombined-bearing Commonwealth exterior cell.

### Completeness Checks
- [ ] **UNSAFE**: N/A.
- [ ] **SIBLING**: Interior walker `walkers.rs:158-204` — parser already correct, just lift the same arms.
- [ ] **DROP**: N/A.
- [ ] **LOCK_ORDER**: N/A.
- [ ] **FFI**: N/A.
- [ ] **TESTS**: Regression test on an FO4 exterior CELL record carrying XCRI/XPRI (e.g. Sanctuary 0,0 or Concord cells). Assert `precombined_mesh_hashes.len() > 0` and `absorbed_refs.len() > 0`.
