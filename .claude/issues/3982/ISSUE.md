# #3982 — REN-2026-09-06-D16-01: #3829's fix closed one of four name-diverging GLSL↔Rust struct mirrors; `ClusterEntry`, `FogClusterEntry` and `CombustionLightMoment` remain outside every lockstep guard

**Labels**: medium, renderer, shaders, bug

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D16-01), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: MEDIUM (defense-in-depth gap on a class with a demonstrated CRITICAL outcome 32 hours ago; not itself a live drift — all three are in sync at HEAD)
- **Dimension**: Volumetrics (GPU-struct lockstep)
- **Location**:
  - `crates/renderer/shaders/volumetrics_inject.comp` — `struct FogClusterEntry`, `struct CombustionLightMoment`, `struct ClusterEntry`
  - `crates/renderer/shaders/cluster_cull.comp` — `struct ClusterEntry`
  - `crates/renderer/shaders/include/bindings.glsl` — `struct ClusterEntry`
  - Rust counterparts: `crates/renderer/src/vulkan/volumetrics.rs` (`GpuFogClusterEntry`, `GpuCombustionLightMoment`), `crates/renderer/src/vulkan/compute.rs` (`ClusterEntry`)
  - The guards that do **not** cover them: `assert_mirror_list_is_complete` / `shader_sources_declaring` and the four lockstep tests that call them (`gpu_instance_glsl_copies_stay_in_lockstep`, `gpu_light_glsl_copies_stay_in_lockstep`, `gpu_water_params_rust_and_glsl_copies_stay_in_lockstep`, `gpu_terrain_tile_glsl_and_rust_fields_stay_in_lockstep`), all in `crates/renderer/src/vulkan/scene_buffer/shader_contract_tests.rs`
- **Status**: **NEW.** No matching open issue (`open_titles.txt` searched for `clusterentry`, `combustion`, `fogcluster`, `mirror`, `lockstep`, `stride` — zero hits). Not a finding in `AUDIT_RENDERER_2026-09-05.md`, which explicitly framed `GpuBoundaryInstance` as *"a sixth mirror sitting entirely outside the tracked set, **not a symptom of a wider pattern**"*. That framing was wrong: the pattern has three more members. `fa5c4191`'s own commit message records the sibling check — *"shader `FogClusterEntry` and `CombustionLightMoment` mirror Rust structs under different names too, and share #3829's discovery blind spot, but both are currently in sync (8 B and 32 B)"* — so the class was **seen and verified once, but not guarded**, and `ClusterEntry` (three GLSL copies) was not part of even that check.
- **Description**: The discovery half of the mirror guard, `shader_sources_declaring(decl)`, matches on a **literal declaration string** (`"struct GpuInstance"`, `"struct GpuLight"`, …). A GLSL struct that mirrors a Rust struct under a *different name* is therefore invisible to it, and the hand-written `SOURCES` tables only list what someone remembered. #3829 was exactly that failure — `GpuBoundaryInstance` went 13 days at a 128 B stride against a 160 B `GpuInstance`, with a green suite, because it wore its own name. The fix added `gpu_boundary_instance_stride_matches_gpu_instance` for that one struct and did not generalise.

  Three name-diverging mirrors remain, none of them covered by any test that reads the GLSL side:

  1. **`CombustionLightMoment` ↔ `GpuCombustionLightMoment`** — eight tightly packed `uint`s, 32 B. The Rust struct's own doc says *"Fixed-point ABI mirrored by `CombustionLightMoment` in `volumetrics_inject.comp`"*, i.e. a stated cross-language ABI. `combustion_light_moment_abi_is_eight_std430_words` asserts **only the Rust side** (`size_of == 32`, `align_of == 4`, `COMBUSTION_LIGHT_GRID_COUNT == 256`); it never `include_str!`s the shader. This one is **field-order sensitive, not just stride sensitive**: the shader writes by name (`atomicAdd(combustionLightMoments[binIndex].weighted_x, …)`, `.radiant_r`, `.luminous_volume`, …) while `decode_combustion_light_moment` decodes **positionally** by word index (`weight: word(0) … luminous_volume: word(7)`). A GLSL-side reorder compiles clean, keeps the size at 32 B, and silently swaps a luma-weighted centroid for a radiant channel.
  2. **`FogClusterEntry` ↔ `GpuFogClusterEntry`** — `{offset, count}`, 8 B. Only a Rust-side `assert_eq!(size_of::<GpuFogClusterEntry>(), 8)` exists. This buffer is now read under the **#3834 partial-upload** contract, where a wrong `count` decode is what makes a stale cluster live.
  3. **`ClusterEntry`** — `{offset, count}`, declared **three times in GLSL** (`cluster_cull.comp` writes it, `include/bindings.glsl` and `volumetrics_inject.comp` read it) against one Rust `#[repr(C)] struct ClusterEntry` in `compute.rs` that only ever appears as `size_of::<ClusterEntry>() * TOTAL_CLUSTERS`. Three GLSL copies with **zero** lockstep coverage — the same multi-copy shape `GpuInstance` and `GpuLight` each have a test for.
- **Evidence**:
  ```
  $ grep -rn "^struct " crates/renderer/shaders/*.{comp,frag,vert} crates/renderer/shaders/include/*.glsl
  → ClusterEntry ×3, FogClusterEntry ×1, CombustionLightMoment ×1, GpuBoundaryInstance ×1,
    GpuFogVolume ×1, GpuInstance ×5, GpuLight ×4, GpuMaterial ×1, GpuTerrainTile ×1, Reservoir ×1
  $ grep -rn "ClusterEntry" crates/renderer/src
  → definition (compute.rs:23) + two size_of uses. No test.
  ```
  `crates/renderer/src/vulkan/reflect.rs` cannot substitute: `uniform_block_size_by_name` reflects **uniform blocks** only. Every struct above lives in an SSBO element array, which the reflector does not size — that is precisely why #3829 needed a source-text test rather than a reflection one.
  Counter-check performed: `GpuFogVolume` **is** covered (`gpu_fog_volume_glsl_field_order_matches_rust_struct`, #2228), so this is a specific gap, not a blanket absence.
- **Impact**: No live corruption today — all three were confirmed field-for-field and size-for-size in sync at HEAD, and the workspace suite is green. The exposure is the next edit: any of these five structs can grow or reorder with a fully green `cargo test`, and the failure mode is the #3829 one — silently wrong data, no validation-layer diagnostic. Blast radius per struct: `CombustionLightMoment` → wrong fire/explosion surface lights (the field-order case is the nastiest, because size stays right); `FogClusterEntry` → wrong local-fog cluster walk under the new partial-upload contract; `ClusterEntry` → wrong clustered-light lists in **both** the fragment shader and the volumetrics inject pass simultaneously.
- **Related**: #3829 (the CRITICAL this class produced, and its one-struct fix), #3231 (the growth that triggered it), #2748 / #3564 (the `GpuInstance` mirror guard + its completeness half), #2228 (the `GpuFogVolume` precedent for a GLSL-reading field-order test), #3834 (the partial-upload contract now riding on `FogClusterEntry`), `feedback_shader_struct_sync.md`.
- **Suggested Fix**: Two steps, the second more valuable than the first.
  1. Add three narrow guards on the `gpu_fog_volume_glsl_field_order_matches_rust_struct` / `gpu_boundary_instance_stride_matches_gpu_instance` pattern — `include_str!` the shader, `parse_glsl_struct_fields_typed`, compare against `parse_rust_struct_fields` (name+order) and `std430_struct_size` (stride). `CombustionLightMoment` needs the **field-order** leg, not just the stride leg, because `decode_combustion_light_moment` is positional. `ClusterEntry` additionally needs its three GLSL copies compared against each other.
  2. Close the discovery gap so a *fifth* name-diverging mirror cannot appear unnoticed: extend `assert_mirror_list_is_complete`'s companion walk to enumerate **every** `^struct ` declaration across `crates/renderer/shaders/` and assert each name appears in a registry of "tracked mirror" or "shader-local, no Rust counterpart" (today's shader-local set: `LocalMedium`, `CombustionDifferential`, `DisneyDiffuseSplit`). That converts "someone remembered" into "someone had to classify it", which is the only version of this guard that survives the next struct.

---

### LOW

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **SPIRV**: If GLSL changed, recompile the `.spv` (`cd crates/renderer/shaders && glslangValidator -V -I. <shader> -o <shader>.spv`) and commit it
- [ ] **TESTS**: A regression test pins this specific fix
