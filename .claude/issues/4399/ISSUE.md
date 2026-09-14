# #4399 — NIFAL-D3-2026-09-14-02: GPU morph-target deformation (#3231) creates its MorphSlot only on entities that can never have `bone_offset != 0`, so it is unreachable end-to-end

**Labels**: medium,nifal,renderer,animation,bug
**Filed from**: `docs/audits/AUDIT_NIFAL_2026-09-14.md` via `/audit-publish` (2026-09-14)

**Source**: `docs/audits/AUDIT_NIFAL_2026-09-14.md` (`/audit-nifal`, HEAD `7374634f5`)

- **Severity**: MEDIUM (feature inert; per-entity GPU weight/delta buffers allocated and refreshed for nothing; no crash)
- **Dimension**: Skinning/Lights
- **Tier Violated**: no-leak (the cell loader uses raw-tier `ImportedMesh.skin.is_some()` as a stand-in for "has a canonical `SkinnedMesh`", which nifal.md documents as never true on that path)
- **Game Affected**: all games with morph-target content on skinned shapes
- **Location**: `byroredux/src/cell_loader/spawn/mesh_instance.rs:1001-1024` (creation), `crates/renderer/src/vulkan/context/build_and_upload_instances.rs:382-384` (read gate)
- **Status**: NEW
- **Description**:
  - **Where slots are created**: the only production `create_morph_slot_for_mesh` call is in `spawn_mesh_instance` (cell loader only), gated on `mesh.skin.is_some()`.
  - **Where slots are read**: only when `bone_offset != 0`, which requires a `SkinnedMesh`. Per #2440, the only production `SkinnedMesh::new_with_global` is `byroredux/src/scene/nif_loader.rs`.
  - **The other path**: the loose-NIF / NPC path produces `bone_offset != 0` but never creates a MorphSlot.

  The two conditions therefore never meet on any entity.
- **Evidence**: Grep results cited above. #3231's verification was a no-regression live boot, not proof of deformation. The only guard, `morph_spawn_uses_mesh_handle_shared_delta_cache`, pins that the call exists, not that it is reachable.
- **Impact**: `AnimatedMorphWeights` are staged into slots no draw consumes, so authored morph animation never deforms anything. The #4294 LRU-recreation and #3661 residency work maintain buffers that are dead on arrival.
- **Related**: #3231, #2440, #4294, #3661, #2221.
- **Suggested Fix**: Gate slot creation on the canonical signal (the entity will carry a `SkinnedMesh`) and add the equivalent creation on the loose-NIF / NPC path where `SkinnedMesh` is built. Add a reachability test. If loose-NIF wiring is out of scope, stop creating cell-path slots and record the gap beside #2440 in nifal.md.
### LOW

## Completeness Checks
- [ ] **DROP**: If MorphSlot / GPU buffer ownership changes, the Drop / LRU release path stays reverse-order correct
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other load paths)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/emitter.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→canonical boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
