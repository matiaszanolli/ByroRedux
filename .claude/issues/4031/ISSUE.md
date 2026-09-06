# #4031 — REN-2026-09-06-D3-08: this dimension's own SKILL instruction now under-counts the `GpuInstance` mirrors, and the one it misses is the one that went wrong

**Labels**: low, renderer, shaders, tech-debt, test-gap, documentation, doc-rot

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D3-08), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW
- **Dimension**: GPU-Struct Layout
- **Location**: `.claude/commands/audit-renderer/SKILL.md` (Dimension 3, the `struct GpuInstance` lockstep bullet)
- **Status**: NEW
- **Description**: The Dimension 3 checklist instructs the auditor to enumerate mirrors with `grep -rlE '^struct GpuInstance' crates/renderer/shaders/`, states the result is **5** declaration sites, and adds that *"the unanchored form returns 6, also matching `skin_vertices.comp`'s comment noting that shader has no `struct GpuInstance`."* Measured today: the anchored grep still returns 5, but the **unanchored form returns 7** — `skin_vertices.comp` plus `volumetrics_inject.comp`, which acquired three `GpuInstance`-naming comments in `fa5c4191` (2026-09-05). More importantly, there are now **6** real mirrors of the struct, not 5: `volumetrics_inject.comp` declares `struct GpuBoundaryInstance`, which reads the very same per-frame `GpuInstance` SSBO — `VolumetricsPipeline::write_boundary_geometry` writes the scene `instance_buffer` into the volumetrics-private descriptor set at binding 19 — and the SKILL's grep recipe cannot see it by construction.
- **Evidence**:
  - `grep -rlE '^struct GpuInstance' crates/renderer/shaders/` → 5 files; unanchored → 7 files.
  - `grep -rhoE "^struct [A-Za-z_]+" crates/renderer/shaders/**` → `struct GpuBoundaryInstance` ×1.
  - `fa5c4191` ("Fix #3829 …"): *"It survived because it is a sixth mirror under its own struct name: outside `gpu_instance_glsl_copies_stay_in_lockstep`'s hardcoded SOURCES list, and invisible to the companion discovery guard, which greps for the literal `struct GpuInstance`."*
  - `.claude/commands/audit-renderer/SKILL.md` was last touched by `a5881e02` and has no mention of `GpuBoundaryInstance`.
- **Impact**: The instruction is a blindfold over exactly the mirror that most recently broke. #3829's `GpuBoundaryInstance` held a 128-byte stride against a 160-byte struct for ~13 days — silently wrong fire/smoke-vs-geometry collision normals, fully green suite — and an auditor following this bullet verbatim would run the anchored grep, get 5, confirm all 5 match, and file "no findings" without ever reaching it. The code side is now guarded (`gpu_boundary_instance_stride_matches_gpu_instance`), so this is a documentation defect rather than a live exposure — but the SKILL is the artifact that decides whether the *next* differently-named mirror gets looked at.
- **Related**: #3829 / `fa5c4191`; #3564 (mirror discovery); the `_audit-common.md` rule "Never write an instruction to not look" (#3199) — the same failure shape, arrived at by rot rather than by authoring.
- **Suggested Fix**: In the Dimension 3 bullet, correct the unanchored count to 7, and replace the "5 declaration sites" framing with "5 sites named `struct GpuInstance` **plus `struct GpuBoundaryInstance` in `volumetrics_inject.comp`**, which reads the same SSBO under a different name and is pinned separately by `gpu_boundary_instance_stride_matches_gpu_instance` (#3829)". Add the struct-name-agnostic recipe (`grep -rhoE '^struct [A-Za-z_]+' crates/renderer/shaders/` and inspect anything binding set 1 / binding 4 or 19) so the count cannot rot the same way again.

---

### Existing — verified still open, do not re-file

| ID | Title | Verification this run |
|---|---|---|
| **#3846** | `bindings.glsl` documents `GpuMaterial` as 396 B and points struct-sync at a nonexistent *gpu_material_size_is_396_bytes* | **STILL UNFIXED.** The header comment above `struct GpuMaterial` in `crates/renderer/shaders/include/bindings.glsl` says *"Mirrors the Rust `GpuMaterial` (396 B std430)"* and *"the size of this struct (396 B) is pinned by `gpu_material_size_is_396_bytes`"*. The real size is 432 B and the real test is `gpu_material_size_is_432_bytes`. `b10a7b7e` / `2853464f` did not reach this file. Report as **Existing**, not new |
| #3909 | `GpuMaterial.texture_index` is an undocumented unsampled lane in the dedup key | Not re-examined (Dim 7 scope); left as-is |
| #3910 / #3911 | supplemental-lane test gaps | Not re-examined (Dim 7 scope); left as-is |

### Checked and clean — no finding

- **All five `struct GpuInstance` GLSL mirrors match the Rust struct field-for-field**, in order, including types (`uvec2 _reserved` ↔ `[u32; 2]`, three scalar `uint`s ↔ three scalar `u32`s — the deliberate anti-`uvec3` shape from #3231 is intact in all five). The `ui.vert` / `water.vert` trap (#785 / #1498) has not recurred.
- **`GpuMaterial`'s single GLSL mirror matches all 108 fields** by name, order and type; all 108 offsets are individually pinned; the dedup hash walks all 108 in declaration order; **no field is `[f32; 3]`** and the struct has no pad fields at all.
- **`GpuInstance`'s pads are explicitly zeroed at both construction sites** (`build_and_upload_instances.rs` sets `_reserved: [0; 2]`, `_reserved2a/b/c: 0`; the UI-quad instance uses `..GpuInstance::default()`), and **no shader reads any pad lane** — the #2164 "live data wearing a padding name" trap has not recurred.
- **All five `uniform CameraUBO` sites are enumerated and lockstep-tested**; `render_debug` remains the appended tail (`uvec4`, not `vec4` — #2688's byte-lethal type-flip class is guarded by `camera_ubo_glsl_copies_stay_in_lockstep`'s typed leg).
- **Capacity constants are in three-way agreement** across code, `memory-budget.md` and `shader-pipeline.md` (table above); the `MAX_INSTANCES < 1 << 24` const-assert guarding the 24-bit `instance_custom_index` is present.
- **Over-cap material intern is safe**: `MaterialTable::intern_by_hash` returns id 0 with a `Once`-gated `warn!` naming `ctx.scratch`; `upload_materials` additionally hard-`assert!`s `len() <= MAX_MATERIALS` in release.
- **The generated-constants path is healthy**: `build.rs` regenerates `shaders/include/shader_constants.glsl` in-tree only on content change; `generated_header_contains_all_defines`, `dbg_bits_catalog_covers_every_dbg_constant`, `instance_flag_bits_match_scene_buffer_consts`, `instance_render_layer_bits_match_scene_buffer_consts`, `material_flag_bits_match_material_consts` and `material_kind_constants_stay_in_lockstep_across_rust_and_glsl` all pass. `bf8ded3d`'s `MESH_ID_*` consolidation left zero hardcoded literals in GLSL.
- **`GpuRayBudget` (`78cc7a41`)**: 17 × `u32` = 68 B Rust-side, 17 fields in the same order in `include/bindings.glsl`, matching `shader-pipeline.md`'s binding-11 row; `RAY_BUDGET_STRIDE = 256` ≥ 68.

### Scope note

This dimension examined `crates/renderer` only — `scene_buffer/{gpu_types,constants,upload,descriptors,ray_budget,material_hash_tests,instance_hash_tests,gpu_instance_layout_tests,shader_contract_tests}.rs`, `vulkan/{material,material_tests,restir,water,volumetrics,gbuffer}.rs`, `shader_constants{,_data}.rs`, `build.rs`, and all GLSL under `crates/renderer/shaders/`. It did not evaluate whether the fields are *used* correctly (Dims 2, 6, 7) or their VRAM cost (Dim 5).

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **SPIRV**: If GLSL changed, recompile the `.spv` (`cd crates/renderer/shaders && glslangValidator -V -I. <shader> -o <shader>.spv`) and commit it
- [ ] **TESTS**: A regression test pins this specific fix
