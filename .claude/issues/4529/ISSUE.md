# REN-6-2026-09-20-02: MaterialTextureHandles producer is copy-paste at nif_loader.rs:1357 and mesh_instance.rs:1165 — unshared and unpinned (the #2444/#2300 duplicate-construction class)

- **ID**: REN-6-2026-09-20-02
- **Labels**: low,renderer,tech-debt,bug
- **Filed from**: docs/audits/AUDIT_RENDERER_2026-09-20.md

**Severity**: LOW · **Dimension**: NIFAL Material
**Source**: docs/audits/AUDIT_RENDERER_2026-09-20.md (REN-6-2026-09-20-02)

**Location**: `byroredux/src/scene/nif_loader.rs:1357` + `byroredux/src/cell_loader/spawn/mesh_instance.rs:1165`

**Description**
The resolve → normal_has_alpha/tint_has_alpha → insert construction is duplicated at two sites with no shared helper and no pin (tests cover only the packer). The same class was closed for Material and emitter overlays; the next channel-presence lane can land asymmetrically here.

**Evidence**
Audit D6, 2026-09-20.

**Impact**
Divergent channel-presence flags between NIF-loaded and REFR-overlaid meshes.

**Suggested Fix**
Extract the producer; one lockstep test over both call sites.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
