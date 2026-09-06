# #4019 — REN-2026-09-06-D2-04: `shader-pipeline.md`'s descriptor table credits `caustic_splat` and `volumetrics` with Set-1 (and bindless Set-0) bindings that their pipeline layouts do not contain — and self-contradicts its own following paragraph

**Labels**: low, renderer, shaders, documentation, doc-rot

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D2-04), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW
- **Dimension**: SSBO/Indexing (authoritative-doc divergence)
- **Location**: `docs/engine/shader-pipeline.md`, the `## Descriptor Sets`
  table — the "Used by" column of rows `0|0`, `0|1`, `1|0`, `1|1`, `1|2`, `1|4`.
  Ground truth: `crates/renderer/src/vulkan/caustic.rs` (the
  `PipelineLayoutCreateInfo` for the caustic compute pipeline) and
  `crates/renderer/shaders/caustic_splat.comp`'s own `layout(set = 0, …)`
  declarations.
- **Status**: **NEW.** Distinct from #3830, which is the *volumetrics binding
  table* further down the same page; this is the Set-0/Set-1 table above it.
  Precedent for the class: `REN-2026-08-30-D2-03` (same table, since fixed).
- **Description**: `CausticPipeline`'s pipeline layout is built with
  `set_layouts(std::slice::from_ref(&partial.descriptor_set_layout))` — **one**
  descriptor set layout, its own private set 0 with bindings 0–10
  (`depthTex`, `normalTex`, `meshIdTex`, its own `LightBuffer`, its own
  `CameraUBO`, its own `InstanceBuffer`, TLAS, `causticAccum`, `CausticParams`,
  and its own `GlobalVertices`/`GlobalIndices`). It binds neither the global
  bindless set 0 nor the scene set 1. The table nonetheless lists
  `caustic_splat` under Set 1 bindings 0, 1 and 4, and `caustic` under Set 0
  bindings 0 and 1.

  The volumetrics entries are worse than merely wrong — they are contradicted
  three paragraphs later by the page's own prose: *"Volumetrics uses its own
  private `set = 0` layout … neither binds any Set-1 resource above."* Yet the
  table credits `volumetrics` with Set-1 bindings 1 and 2 and Set-0 binding 0.
- **Evidence**: `grep -n "set_layouts(" crates/renderer/src/vulkan/caustic.rs`
  → a single-element slice at the pipeline-layout site.
  `grep -n "set = 1, binding" crates/renderer/shaders/caustic_splat.comp` →
  nothing; every declaration in that file is `set = 0`. `caustic_splat.comp`
  declares its own `struct GpuInstance` and its own `GlobalVertices`/
  `GlobalIndices` precisely *because* it cannot see set 1 — which is what its
  own header comment ("caustic uses its own descriptor set") says.
- **Impact**: Audit-methodology and onboarding only, no runtime effect — but
  this is the table every audit is instructed to prefer over re-deriving
  descriptor facts from source, and it is the table that answers "which
  pipelines must be re-bound when Set 1 changes". An engineer widening a Set-1
  binding would look here and conclude that `caustic_splat` and `volumetrics`
  need updating (they don't), or — the more dangerous direction — that
  `caustic_splat`'s instance reads are covered by the Set-1 lockstep guards
  (they aren't; that is a separate private mirror, and it is the class of
  mistake that produced #3829).
- **Related**: #3830 (`volumetrics_inject.comp` binding table on the same
  page); `REN-2026-08-30-D2-03` (the previous divergence in this same table);
  #3829 (the CRITICAL that turned on exactly this "which pipelines actually see
  Set 1" question).
- **Suggested Fix**: Correct the six "Used by" cells to name only the pipelines
  whose `VkPipelineLayout` actually includes that set, and add a one-line note
  to the caustic row equivalent to the volumetrics one ("`caustic_splat.comp`
  uses its own private `set = 0` layout with its own `GpuInstance` mirror and
  vertex/index SSBOs"). The `shader_contract_tests.rs` `include_str!`-the-doc
  pattern (`froxel_grid_cost_matches_the_memory_budget_doc`) could pin the
  caustic row against `caustic.rs`'s set count.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **SPIRV**: If GLSL changed, recompile the `.spv` (`cd crates/renderer/shaders && glslangValidator -V -I. <shader> -o <shader>.spv`) and commit it
- [ ] **TESTS**: A regression test pins this specific fix
