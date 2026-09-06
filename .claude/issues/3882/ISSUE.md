# #3882: TD7-2026-09-05-04: `shader_constants_data.rs` hand-copies `MAX_BONES_PER_MESH = 144` where its own re-export pattern and an existing build-dependency allow a derivation

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD7-2026-09-05-04) via `/audit-publish`, 2026-09-05. Labels: `low,renderer,shaders,tech-debt,bug`.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3882 --json state`.

---

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD7-2026-09-05-04), `/audit-tech-debt` full 9-dimension sweep at `fa5c4191`. Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.



- **Severity**: LOW
- **Dimension**: 7 — Magic Numbers & Hardcoded Constants
- **Location**: `crates/renderer/src/shader_constants_data.rs::MAX_BONES_PER_MESH` (source of truth: `crates/core/src/ecs/components/skinned_mesh.rs::MAX_BONES_PER_MESH`)
- **Status**: NEW
- **Effort**: trivial (≤30 min)
- **Description**: `shader_constants_data.rs` exists precisely so a shared constant has one definition, and it already resolves ~40 of its entries by re-export — `WORLD_UNITS_PER_METER`, `LEGACY_LIGHT_CULL_RANGE_MULTIPLIER`, every `COMBUSTION_*` / `FLAME_*` / `EXPLOSION_*`, every `VISIBILITY_LAYER_*`, `WATER_*`, `DEFAULT_GLASS_BLUR_SCALE`, `DEFAULT_WATER_WAVE_AMPLITUDE` — all written as `byroredux_core::…`. `MAX_BONES_PER_MESH` is the outlier: it restates the literal `144` and points at core only in a comment (*"see `byroredux_core::ecs::components::skinned_mesh::MAX_BONES_PER_MESH` for the vanilla-content survey that fixes this ceiling at 144"*).
- **Evidence**:
  - `crates/core/src/ecs/components/skinned_mesh.rs` declares `pub const MAX_BONES_PER_MESH: usize = 144;`, publicly re-exported by `crates/core/src/ecs/components/mod.rs` (`pub use skinned_mesh::{SkinnedMesh, MAX_BONES_PER_MESH};`) — the exact path the doc comment names.
  - `crates/renderer/Cargo.toml` lists `byroredux-core` under `[build-dependencies]`, so `build.rs`'s `include!("src/shader_constants_data.rs")` resolves `byroredux_core::…` paths — proven by the ~40 sibling entries that already do.
  - The only guard is `shader_constants.rs::max_bones_per_mesh_matches_core`, a runtime `assert_eq!` in a `#[cfg(test)]` module. A re-export would make the same fact a compile-time identity.
  - **Deliberately not flagged as a sibling**: `VERTEX_STRIDE_FLOATS = 26` looks like the same defect but is *forced* — its source of truth is `crate::vertex::Vertex`, and `build.rs` cannot import the crate it builds. Its `size_of::<Vertex>()` test is the correct mitigation there, not a workaround.
- **Impact**: minimal in practice (the test catches divergence), but it is one hand-maintained copy of a survey-derived ceiling in the one file whose entire purpose is to not have those. Editing core's value and running only `-p byroredux-core` leaves the shader header stale until the renderer's test suite runs.
- **Related**: #1758 / TD7-001 (`SKIN_WORKGROUP_SIZE` — the sibling skinning constant, fixed the same way) · #1451 / SKIN-02 · TD7-2026-09-05-02
- **Suggested Fix**: `pub const MAX_BONES_PER_MESH: u32 = byroredux_core::ecs::components::MAX_BONES_PER_MESH as u32;`, matching the surrounding re-export style. Keep `max_bones_per_mesh_matches_core` — it becomes trivially true, which is the desired end state.

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
- [ ] **DROP**: If Vulkan objects change, the Drop impl stays reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
