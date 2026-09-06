# #4018 — REN-2026-09-06-D2-03: the unsafe-vertex-lane guard covers 2 of the 4 shaders that read a raw-float vertex SSBO, with no completeness half — the same shape as #3829

**Labels**: low, renderer, shaders, test-gap, bug

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D2-03), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW
- **Dimension**: SSBO/Indexing (test gap)
- **Location**: `crates/renderer/src/vulkan/scene_buffer/shader_contract_tests.rs`
  — `rt_hit_shaders_have_no_unsafe_vertex_data_reads` and its `sources` array.
  Uncovered readers: `crates/renderer/shaders/caustic_splat.comp`
  (`getCausticHitTriWorldPositions`, its own `GlobalVertices` at set 0 binding 9)
  and `crates/renderer/shaders/volumetrics_inject.comp` (`boundaryVertexData`,
  binding 20). The invariant it enforces is documented on the `GlobalVertices`
  block in `crates/renderer/shaders/include/bindings.glsl` (#575 / SH-1).
- **Status**: **NEW.** `docs/audits/AUDIT_RENDERER_2026-08-14.md` names the test
  as present but does not examine its source list. No open issue matches.
- **Description**: The `Vertex` layout has six float lanes that are **not**
  IEEE-754 floats — bone indices (12–15, `u32` bits) and splat weights (20–21,
  packed 4× `u8` unorm) — and reading them as `vertexData[base + N]` yields
  NaN/denormal garbage. `rt_hit_shaders_have_no_unsafe_vertex_data_reads` is the
  static guard for that, and its `sources` array is a hardcoded two-entry list:
  `triangle.frag` and `include/ray_hit.glsl`. Two other shaders index a raw
  `float …[]` vertex SSBO with `VERTEX_STRIDE_FLOATS`-style arithmetic and are
  outside it. Unlike its sibling `gpu_instance_glsl_copies_stay_in_lockstep`,
  which pairs its hardcoded `SOURCES` with `assert_mirror_list_is_complete`,
  this test has **no discovery half** — nothing fails when a new shader starts
  reading the vertex buffer.

  **No live unsafe read exists today.** `caustic_splat.comp` reads only lanes
  0–2 (position) plus its skinned `buffer_reference` output;
  `volumetrics_inject.comp` reads only `base0..base0+2`. `triangle.vert` and
  `skin_vertices.comp` reach bone indices through `floatBitsToUint`, correctly.
  This is a coverage gap, not a defect — but it is structurally identical to
  #3829, where a hardcoded shader list with no completeness half let
  `volumetrics_inject.comp` (the same file) drift outside a GPU-layout contract
  for 13 days and cost a CRITICAL.
- **Evidence**: The test body's `let sources = [("triangle.frag", …),
  ("ray_hit.glsl", …)];` — two entries, and no `include_dir`/discovery
  assertion anywhere in the function. `grep -rln "vertexData\[" crates/renderer/shaders`
  → `caustic_splat.comp`, `include/ray_hit.glsl`, `include/bindings.glsl`;
  `grep -rn "float vertexData\[\]\|float boundaryVertexData\[\]" ` adds
  `volumetrics_inject.comp`.
- **Impact**: A future hit-fetch site added to `caustic_splat.comp` or
  `volumetrics_inject.comp` — e.g. reading a per-vertex splat weight for a
  terrain-aware caustic or boundary material — would silently reinterpret a
  packed `u32` as a float and produce NaN, with the guard green. The failure
  mode is exactly the one the `bindings.glsl` WARNING block calls "the
  pit-of-failure guardrail".
- **Related**: #575 / SH-1 (the invariant); #3829 /
  `REN-2026-09-05-D2-01` (the same hardcoded-list-without-a-completeness-half
  shape, same file, CRITICAL outcome); `gpu_boundary_instance_stride_matches_gpu_instance`
  and `assert_mirror_list_is_complete` (the two patterns that do it right).
- **Suggested Fix**: Replace the hardcoded pair with a recursive walk of
  `crates/renderer/shaders/` (`.frag`/`.vert`/`.comp`/`.glsl`) that scans every
  file declaring or indexing a raw-float vertex SSBO — `78cc7a41` already built
  exactly that walker for the #3880 constant gate, so the traversal can be
  reused rather than rewritten. Failing that, add
  `caustic_splat.comp` + `volumetrics_inject.comp` to `sources` **and** a
  completeness assertion that every file matching `float \w*[Vv]ertex\w*\[\]` is
  in the list.

---

## Completeness Checks

- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **SPIRV**: If GLSL changed, recompile the `.spv` (`cd crates/renderer/shaders && glslangValidator -V -I. <shader> -o <shader>.spv`) and commit it
- [ ] **TESTS**: A regression test pins this specific fix
