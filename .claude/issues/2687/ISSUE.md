# SAFE-D9-01: Save-restore Material producer runs neither resolve_pbr nor finiteness validation

**Issue**: #2687
**Filed**: 2026-08-12 via `/audit-publish` from `/audit-suite renderer-deep`

- **Severity**: LOW
- **Dimension**: 9 — NIFAL boundary, NaN/Inf on the GPU (safety facet)
- **Location**: [save_io.rs](byroredux/src/save_io.rs) (`build_save_registry`, `.register_component::<Material>("Material")`) · `crates/save/src/{validate,snapshot,driver}.rs` · consumed by [static_meshes.rs](byroredux/src/render/static_meshes.rs)
- **Status**: NEW (persistence half of the OPEN #2489; #2489's scope is the `mat.set` write site only)
- **Description**: `Material` became a save-registered component under #2378 so
  live `mat.set` edits survive a round trip. `restore_world` inserts the decoded
  `Material` straight into the ECS; nothing on that path calls
  `Material::resolve_pbr()` (the *only* NaN detector for `metalness`/`roughness`,
  both plain `f32`) and there is no finiteness gate anywhere in the save crate —
  `grep -rn "is_finite\|is_nan" crates/save/src byroredux/src/save_io.rs` returns
  nothing. The renderer then reads `m.roughness` / `m.metalness` /
  `m.ior` directly in `static_meshes.rs` and interns them into the SSBO.
- **Evidence**: `translate_material` is the only production `resolve_pbr()`
  caller ([material_translate.rs](byroredux/src/material_translate.rs)); the restore path bypasses
  translation entirely (M45.1 applies FormId-keyed deltas *after* cell reload,
  so a restored `Material` overwrites the translated one).
- **Impact**: A non-finite scalar that reaches a save — today only reachable via
  `mat.set nan` (#2489), or via a hand-edited/corrupt save file — is replayed
  into the material SSBO on every subsequent load, with no re-sanitisation.
  NaN on the GPU is UB. Low severity because injection requires deliberate
  console input or file tampering, but note that fixing #2489 alone does **not**
  close this leg: an already-poisoned save still restores raw.
- **Related**: #2489 (OPEN — `mat.set` has no clamp/finite guard), #2378 (CLOSED
  — registered `Material`), #1409/#1411/#1434/#1443 (the finite-guard family).
- **Suggested Fix**: Call `resolve_pbr()` (or a dedicated `sanitize_finite()`)
  on each `Material` after `restore_world`, or add a finiteness sweep to the
  save-side validate gate so a poisoned snapshot is rejected before it is
  written.

---


---
*Filed from [`docs/audits/AUDIT_SAFETY_2026-08-12.md`](docs/audits/AUDIT_SAFETY_2026-08-12.md) — `/audit-suite renderer-deep`, 2026-08-12. Finding ID `SAFE-D9-01`.*

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix
