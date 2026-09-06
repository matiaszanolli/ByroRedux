# #3868: TD3-2026-09-05-04: `triangle.frag` describes R1 Phase 6 as still pending in three present-tense comments, and attributes the UV/alpha identity defaults to `GpuInstance::default()` — a struct that has carried none of those fields since 2026-05-01

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD3-2026-09-05-04) via `/audit-publish`, 2026-09-05. Labels: `low,shaders,renderer,doc-rot,documentation`.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3868 --json state`.

---

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD3-2026-09-05-04), `/audit-tech-debt` full 9-dimension sweep at `fa5c4191`. Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.



- **Severity**: LOW
- **Dimension**: 3 — Stale Documentation & Comments
- **Location**: `crates/renderer/shaders/triangle.frag:213-218`, `:226-231`, `:376-384`
- **Status**: NEW
- **Effort**: trivial (≤30 min)
- **Age**: R1 (MaterialTable refactor) closed **2026-05-01** across 6 phases (`aa48d64`..`22f294a`) per `ROADMAP.md`. **~4 months stale.**
- **Description**: Three comment blocks in the main fragment shader narrate the R1 migration as in-flight. The exact stale sentences:
  1. `:216-218` — *"The legacy per-instance copies on `GpuInstance` **are still populated** by the CPU pipeline **(Phase 6 drops them)** and are byte-equal to `mat.*`, so the visible output is unchanged."*
  2. `:379-383` — *"The per-instance `inst.roughness` slot **is still populated** by the CPU pipeline (Phase 6 drops it once every reader has migrated); the value at `materials[inst.materialId].roughness` is byte-equal to it **for now** … **Phases 5 and 6 migrate the remaining per-material fields one slice at a time.**"*
  3. `:228-230` — *"Identity defaults (offset=(0,0), scale=(1,1), alpha=1.0) come from **`GpuInstance::default()`**"* — attached directly above a line that reads `mat.uvScaleU` / `mat.uvOffsetU`.
- **Evidence**: The live `GpuInstance` (`scene_buffer/gpu_types.rs`) is `model`, `texture_index`, `bone_offset`, `vertex_offset`, `index_offset`, `vertex_count`, `flags`, `material_id`, `ior`, `avg_albedo_r/g/b`, `surface_id`, `skinned_vertex_address`, `_reserved`, `morph_delta_address`, `morph_weight_address`, `morph_target_count`, `_reserved2a/b/c`. There is **no** `roughness`, `metalness`, `emissive_*`, `specular_*`, `alpha_threshold`, `uv_offset_*`, `uv_scale_*` or `material_alpha` field — Phase 6 completed. The identity defaults the comment attributes to `GpuInstance::default()` actually live in `impl Default for GpuMaterial` (`crates/renderer/src/vulkan/material.rs`):
  ```rust
  material_alpha: 1.0,
  uv_offset_u: 0.0, uv_offset_v: 0.0,
  uv_scale_u: 1.0,  uv_scale_v: 1.0,
  ```
  `GpuInstance`'s own `Default` impl contains no `uv_*` or `alpha` field at all.
- **Impact**: Anyone reading the shader to understand where a per-material value comes from is told two mutually reinforcing falsehoods: that a redundant per-instance copy exists and is authoritative-equal, and that the correct place to look for a default is `GpuInstance`. Both claims would send a contributor hunting for fields on the wrong struct — the same confusion `/audit-renderer`'s recurring "#785 trap" (`ui.vert` reading `textureIndex` not `materialId`) exists to guard against. `docs/audits/AUDIT_RENDERER_2026-05-01.md` (R1-N1) already documents the retained-field exceptions as a live hazard; leaving the shader claiming *more* retentions than exist compounds it. No runtime effect — `mat.*` is what the shader actually reads.
- **Related**: `ROADMAP.md` R1 row (closed 2026-05-01), #785, #2045 (CLOSED — a different `triangle.frag` doc/constant defect).
- **Suggested Fix**: Reframe both `:216-218` and `:379-383` in the past tense — R1 Phases 4-6 are closed and `GpuInstance` no longer carries per-material copies; delete the "byte-equal … for now" and "Phases 5 and 6 migrate …" clauses. At `:229`, change `` `GpuInstance::default()` `` → `` `GpuMaterial::default()` ``.

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
- [ ] **DROP**: If Vulkan objects change, the Drop impl stays reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
