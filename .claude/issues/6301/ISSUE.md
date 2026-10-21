# #6301: NIF-D4-2026-10-21-01: #5792's one-gate consolidation stops at `walk/mod.rs`; the satellite walkers re-spell the node gate, and the particle-emitter walker omits the editor-marker half of it

**Labels**: low,bug,nif-parser,nif,tech-debt,test-gap

**Source**: docs/audits/AUDIT_NIF_2026-10-21.md · **Severity**: LOW · **Dimension**: Geometry Extraction & Import Handoff · **Location**: `crates/nif/src/import/walk/{lights,texture_effect,emitter}.rs`; guard `crates/nif/src/import/tests/app_culled_visibility.rs` (`the_shape_cull_gate_is_spelled_once`)

- **Game Affected**: all
- **Location**: `crates/nif/src/import/walk/lights.rs:40-45,54-59`, `crates/nif/src/import/walk/texture_effect.rs:41-46,55-60`, `crates/nif/src/import/walk/emitter.rs:798,823`; guard `crates/nif/src/import/tests/app_culled_visibility.rs:158` (`the_shape_cull_gate_is_spelled_once`)
- **Status**: NEW (follow-through of #5792, closed)
- **Description**: `ef78a0eec` moved the hierarchical and flat walkers onto `shape_is_culled` and `node_is_culled` (`walk/mod.rs:278-290`). Its source-count guard scans only `walk/mod.rs`. The three satellite walkers still re-spell the node gate inline. `walk_node_lights` and `walk_node_texture_effects` spell all of it (`flags & 0x01` plus `is_editor_marker`). `walk_node_particle_emitters_flat` checks only `node.av.flags & 0x01 != 0` on its switch and `NiNode` arms. It therefore descends into an `EditorMarker…`/`MarkerX`/`marker_…` node that the main walkers skip, and imports a particle system under that node whose own name and flags pass `particle_system_is_culled`.
- **Evidence**: `emitter.rs:798` and `:823` contain `if node.av.flags & 0x01 != 0 { return; }` with no `is_editor_marker` sibling. `lights.rs:43` and `texture_effect.rs:44` do have it. The `census` probe found **0** visible `NiParticleSystem`s under a non-culled editor-marker node on Oblivion (547), FO3 (422), FNV (1,262), SSE (1,704), FO4 (1,345) and Starfield (0 systems).
- **Impact**: No effect on vanilla content. A mod that authors an emitter under an editor-marker helper node would spawn its particles while that node's geometry siblings stay hidden. More importantly, this is the exact failure #5792 was filed to prevent: the next node-gate rule change has three more sites to hand-edit, and the guard cannot see them.
- **Related**: #5792, #4561, #3856.
- **Suggested Fix**: Route the four satellite node arms through `node_is_culled`, which is already `pub(super)` in the same module tree. Extend `the_shape_cull_gate_is_spelled_once` so it also scans `lights.rs`, `texture_effect.rs` and `emitter.rs`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
