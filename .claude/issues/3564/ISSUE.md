# #3564 — REN-2026-08-30-D3-02: every GLSL-mirror lockstep guard drives off a hardcoded `SOURCES` list, so a newly-added mirror site is silently unguarded

**Labels**: `medium,renderer,shaders,test-gap,bug`
**Filed**: 2026-08-30 via `/audit-publish`
**Report**: `docs/audits/AUDIT_RENDERER_2026-08-30.md`

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is
> authoritative for current state — `gh issue view 3564 --json state`.

---

- **Severity**: MEDIUM
- **Dimension**: GPU-Struct Layout
- **Location**: `crates/renderer/src/vulkan/scene_buffer/shader_contract_tests.rs` (`gpu_instance_glsl_copies_stay_in_lockstep`, `gpu_light_glsl_copies_stay_in_lockstep`, `gpu_terrain_tile_glsl_and_rust_fields_stay_in_lockstep`, `gpu_water_params_rust_and_glsl_copies_stay_in_lockstep`), `crates/renderer/src/vulkan/reflect.rs` (`camera_ubo_size_matches_gpu_camera_in_every_shader`)
- **Status**: New
- **Description**: Hand-mirrored GLSL struct declarations are the documented #1
  silent-desync source in this codebase, and the guards built for them
  (#1916, #2748) work well — but each one hardcodes the *set of files it checks* as a
  `const SOURCES: &[(&str, &str)]` of `include_str!` literals. Nothing scans
  `crates/renderer/shaders/` for declarations. A new shader that declares
  `struct GpuInstance` (or `GpuLight`, or a `CameraUBO` block) is therefore born
  completely outside the lockstep contract, and every existing test stays green.
- **Evidence**: `gpu_instance_glsl_copies_stay_in_lockstep`
  (`shader_contract_tests.rs:1751`) hardcodes 5 paths;
  `gpu_light_glsl_copies_stay_in_lockstep` (`:1682`) hardcodes 4;
  `camera_ubo_size_matches_gpu_camera_in_every_shader` (`reflect.rs:606`) hardcodes 6
  `.spv` and carries the tell in its own comment:

  > `// Every shader that declares the `CameraUBO` block. Add new readers here so they are pinned too.`

  The guard's correctness is delegated to a code-review convention, which is precisely
  what the guard was introduced to stop relying on.

  **The lists are currently complete** — verified independently, so this is latent,
  not a live desync:
  * `struct GpuInstance` → 5 real declarations (the 6th `grep -rl` hit,
    `skin_vertices.comp`, is a comment).
  * `struct GpuLight` → 4 declarations, matching the 4 hardcoded.
  * `CameraUBO` → exactly 6 committed `.spv` contain the block
    (`strings -a *.spv | grep '^CameraUBO$'`), matching the 6 hardcoded.

  The near-miss is already on record: `skin_vertices.comp` reads
  `morph_delta_address` / `morph_weight_address` / `vertex_count` — three fields that
  mirror `GpuInstance` semantics — through a hand-written `layout(push_constant)`
  block, and its own source comment (`skin_vertices.comp:82-84`) states the parity is
  *"not covered by an automated parity test, since this shader has no
  `struct GpuInstance` for the existing GpuInstance-lockstep tests to anchor on."*
  (That particular block does currently match `SkinPushConstants` field-for-field and
  is size-pinned at 32 B, so it is correct today — but it is correct unguarded.)
- **Impact**: The strongest structural guarantee in the renderer's GPU-contract
  test suite has a discovery hole. The failure is silent by construction: a 6th
  `GpuInstance` mirror with a dropped or reordered field produces garbage
  transforms / texture indices / morph addresses for whatever pass reads it, with a
  fully green `cargo test`. Given `GpuInstance` grew twice in recent history
  (#2219 128 B, #3231 160 B) and each growth touched every mirror, the probability of
  a new consumer appearing is non-trivial.
- **Suggested Fix**: Replace the hardcoded lists with discovery. Tests can read the
  filesystem (`CARGO_MANIFEST_DIR` is set): walk `crates/renderer/shaders/**/*.{vert,frag,comp,glsl}`,
  collect every file containing `struct GpuInstance` (excluding comment-only hits —
  the existing `extract_struct_body` helper already distinguishes them), and assert
  the discovered set **equals** the expected set before running the field compare.
  That converts "someone forgot to add the file" from silent to a named test
  failure. Apply the same to `GpuLight`. For `camera_ubo_size_…`, iterate every
  committed `.spv` and pin any that reflects a `CameraUBO` block, rather than
  enumerating six by hand.

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-08-30.md` — REN-2026-08-30-D3-02

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
