# SKY-2026-08-27-D7-02: `material_translate.rs`'s Phase-2 module doc claims Skyrim roughness ships from Phase 2, which the Phase-2 function's own rule makes impossible

Labels: low,nifal,documentation,doc-rot,game:skyrim,legacy-compat

- **Severity**: LOW
- **Confidence**: CONFIRMED (code read; the two statements are mutually exclusive)
- **Location**: `byroredux/src/material_translate.rs:50-55` (claim) vs
  `byroredux/src/material_translate.rs:719-777` (`normal_alpha_spec_roughness`,
  the `if normal_has_alpha { None }` arm at :770-771)
- **Description**: The module header states:

  > *"This matters for Skyrim in particular: it has no dedicated gloss map and
  > its spec mask lives in the normal-map alpha, so for most Skyrim architecture
  > the shipped roughness comes from Phase 2, not from Phase 1's literal. Anyone
  > adding per-game material logic needs to know both write sites exist — a
  > Phase-1-only change will not stick for a field that Phase 2 also writes."*

  `resolve_normal_alpha_spec_roughness` is the only Phase-2 writer of
  `Material::roughness`, and it delegates to `normal_alpha_spec_roughness`,
  whose first branch is:

  ```rust
  if normal_has_alpha {
      None
  } else if metalness < 0.3 && env_map_scale <= 0.3 && specular_strength > 1.2 {
  ```

  with its own doc saying *"An alpha-bearing normal is deliberately a no-op here:
  its alpha is the per-pixel specular-intensity mask consumed in the shader,
  never a smoothness signal."* The header's own premise — Skyrim's spec mask
  lives in the normal-map alpha — is precisely the condition under which
  Phase 2 returns `None` and Phase 1's value ships. What Phase 2 does for that
  population is the *gloss-slot binding* (`normal_alpha_spec_binding_applies`,
  render-side), not a roughness write.
- **Evidence**: quoted above; the two doc blocks are 720 lines apart in the same
  file and assert opposite ownership of `Material::roughness` for the same
  population.
- **Impact**: The one paragraph in the codebase that tells a future contributor
  *where to change Skyrim roughness* points at the wrong write site. A
  Phase-1-only change to Skyrim architecture roughness in fact does stick; a
  Phase-2 change to it is a no-op. No runtime effect.
- **Suggested Fix**: correct the paragraph — for alpha-bearing Skyrim normals
  Phase 1 owns the scalar and Phase 2 owns only the per-draw gloss-slot binding;
  Phase 2's roughness write is the alpha-*less*, high-`specular_strength`
  fallback.
- **Related**: #1480 (the resolve-once relocation this doc describes), #2330
  (two-phase boundary). Not covered by #3188/#3236 (both CLOSED, and both about
  `nifal.md`, not this module header).

---

---

*Filed from `docs/audits/AUDIT_SKYRIM_2026-08-27.md` (`/audit-skyrim`, 7 dimensions),
verified against HEAD `558af58c` on a full vanilla Skyrim SE install.*
