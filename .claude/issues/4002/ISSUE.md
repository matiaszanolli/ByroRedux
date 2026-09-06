# #4002 — REN-2026-09-06-D10-02: `dof_effective_view_proj` applies the lens jitter in ABSOLUTE space, so the aperture offset is quantised away at exterior magnitudes

**Labels**: low, renderer, shaders, bug

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D10-02), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW (the DoF path has no production enabler today)
- **Dimension**: Camera-Relative Precision
- **Location**: `crates/renderer/src/vulkan/context/draw.rs` (`dof_effective_view_proj`)
- **Status**: NEW
- **Description**: The function correctly returns a **render-origin-relative** matrix — its
  own doc says so and `look_at_rh(jittered_eye − render_origin, focal_pt − render_origin, up)`
  delivers it. But the two points it subtracts from are *composed* at absolute magnitude
  first: `jittered_eye = pos + lens_u * right + lens_v * up` and
  `focal_pt = pos + focus_dist * fwd`, with `pos` the raw absolute camera position. The
  aperture term is a sub-unit lens offset being added to a value whose f32 ULP, at
  Markarth's X ≈ −176 000, is 0.015625 — so the jitter is quantised (and, below ~0.008 u,
  discarded outright) *before* the rebase that was supposed to protect it. Doing the
  rebase first — `let rel = pos − render_origin;` then composing from `rel` — is exact and
  costs nothing.
- **Evidence**: `jittered_eye` / `focal_pt` are built from `Vec3::from_array(camera_pos)`
  (absolute) and only rebased inside the `look_at_rh` call. The subtraction itself is exact
  (`render_origin` is a multiple of 4096, and the difference is < 4096, hence exactly
  representable) — the loss happens strictly in the addition that precedes it.
- **Impact**: **Currently dormant, and I could not find a way to reach it.** `Camera::aperture`
  defaults to `0.0`, no console command or production code path sets it (the only non-zero
  values in the tree are `2.5` inside `draw.rs`'s own tests), and `fsr_gated_dof` forces
  `aperture = 0.0` whenever FSR — the engine-default upscaler — is active. If DoF is ever
  wired up, a subtle aperture (≲ 0.05 u) would produce ~3 distinct sample offsets instead
  of a smooth disk at exterior worldspaces, i.e. banding rather than bokeh; a large one
  (2.5 u) would still work, coarsely. Reported because it is the one place in the
  render-origin machinery where the rebase happens later than it needs to, and the fix is
  a two-line reorder that removes the question permanently.
- **Related**: #1525 (the degenerate-`focus_dist` guard, intact), #2197 (`fsr_gated_dof`),
  `docs/engine/shader-pipeline.md` §"Render-origin-relative (raster path)".
- **Suggested Fix**: Hoist the rebase: `let rel = Vec3::from_array(camera_pos) - render_origin;`
  then build `jittered_eye_rel = rel + lens_u * right + lens_v * up` and
  `focal_pt_rel = rel + focus_dist * fwd`, passing those to `look_at_rh` directly. Keep
  returning the **absolute** `jittered_eye` (`rel + render_origin`) since the shader's
  view-dir math wants it, as the current doc comment already specifies.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
